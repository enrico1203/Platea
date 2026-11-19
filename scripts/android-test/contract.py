#!/usr/bin/env python3
"""Prova del contratto tra l'app Android e il server.

Avvia un server Platea di prova in un container (dati temporanei, solo i file di prova di ./media in sola lettura),
fa la procedura di primo avvio, crea le librerie e salva in una cartella le risposte JSON di ogni endpoint che l'app
usa in `Android/app/src/test/resources/contract/` (token tolti). Poi i test JVM dell'app
(`scripts/gradle.sh testDebugUnitTest`) le leggono con i modelli dell'app. Controlla anche le decisioni del server con profili tipici di telefoni e televisori.

    python3 scripts/android-test/contract.py --work /tmp/at [--image ghcr.io/enrico1203/platea-server:latest]
"""

import argparse
import json
import re
import os
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request

NAME = "platea-android-contract"
PORT = 18196
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
passed, failed = [], []


def check(name, cond, detail=""):
    (passed if cond else failed).append(name)
    print(("  OK  " if cond else "  KO  ") + name + (f"  [{detail}]" if detail and not cond else ""), flush=True)
    return cond


class Api:
    def __init__(self, base):
        self.base = base
        self.token = None

    def req(self, method, path, body=None):
        h = {"Accept": "application/json"}
        if self.token:
            h["Authorization"] = f"Bearer {self.token}"
        data = None
        if body is not None:
            h["Content-Type"] = "application/json"
            data = json.dumps(body).encode()
        r = urllib.request.Request(self.base + path, data=data, method=method, headers=h)
        try:
            with urllib.request.urlopen(r, timeout=60) as resp:
                c = resp.read()
                return resp.status, (json.loads(c) if c and resp.headers.get_content_type() == "application/json" else c)
        except urllib.error.HTTPError as e:
            c = e.read()
            try:
                return e.code, json.loads(c)
            except ValueError:
                return e.code, c


def sh(*a):
    return subprocess.run(list(a), capture_output=True, text=True)


def wait(fn, timeout, what):
    end = time.time() + timeout
    while time.time() < end:
        try:
            v = fn()
        except OSError:
            v = None
        if v:
            return v
        time.sleep(0.5)
    raise RuntimeError(f"tempo scaduto: {what}")


# Profili tipici, come li costruisce `Profile.kt` sui dispositivi.
PHONE = {
    "name": "Platea Android", "containers": ["mp4", "m4v", "mov", "mkv", "matroska", "webm", "ts", "mpegts"],
    "video_codecs": [
        {"codec": "h264", "profiles": ["baseline", "constrained baseline", "main", "high"], "max_bit_depth": 8, "max_width": 4096, "max_height": 2304},
        {"codec": "hevc", "profiles": ["main", "main 10"], "max_bit_depth": 10, "max_width": 4096, "max_height": 2304},
        {"codec": "vp9", "max_bit_depth": 10},
    ],
    "audio_codecs": [{"codec": "aac", "max_channels": 8}, {"codec": "mp3", "max_channels": 2}, {"codec": "opus", "max_channels": 8}, {"codec": "flac", "max_channels": 8}],
    "subtitle_text": ["vtt"], "subtitle_image": [], "hdr": [], "max_bitrate": None, "protocols": ["file", "hls"],
    "hls_video_codecs": ["h264", "hevc"], "hls_audio_codecs": ["aac", "mp3"],
}
# Fire TV Stick vecchio: solo H.264, AC3 in passthrough verso il televisore.
FIRE_OLD = dict(PHONE, name="Platea Android TV",
                video_codecs=[{"codec": "h264", "profiles": ["baseline", "constrained baseline", "main", "high"], "max_bit_depth": 8, "max_width": 1920, "max_height": 1080}],
                audio_codecs=PHONE["audio_codecs"] + [{"codec": "ac3", "max_channels": 6}, {"codec": "eac3", "max_channels": 8}],
                hls_video_codecs=["h264"], hls_audio_codecs=["aac", "mp3", "ac3", "eac3"])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--work", required=True)
    ap.add_argument("--image", default="ghcr.io/enrico1203/platea-server:latest")
    ap.add_argument("--keep", action="store_true")
    ap.add_argument("--fixtures", default=os.path.join(ROOT, "Android/app/src/test/resources/contract"),
                    help="dove salvare le risposte per i test JVM dell'app")
    a = ap.parse_args()
    work = os.path.abspath(a.work)
    fx = os.path.abspath(a.fixtures)
    shutil.rmtree(fx, ignore_errors=True)
    os.makedirs(fx)
    shutil.rmtree(work, ignore_errors=True)
    for d in ("data", "cache", "transcode", "tv"):
        os.makedirs(os.path.join(work, d))
    make_tv(work, a.image)
    os.chmod(work, 0o777)
    for d in ("data", "cache", "transcode", "tv"):
        os.chmod(os.path.join(work, d), 0o777)
    sh("docker", "rm", "-f", NAME)
    r = sh("docker", "run", "-d", "--name", NAME, "-p", f"{PORT}:8096",
           "-v", f"{work}/data:/data", "-v", f"{work}/cache:/cache", "-v", f"{work}/transcode:/transcode",
           "-v", f"{ROOT}/media/Film:/media/Film:ro", "-v", f"{ROOT}/media/Serie:/media/Serie:ro", "-v", f"{work}/tv:/media/tv:ro", a.image)
    if r.returncode != 0:
        sys.exit(r.stderr)
    api = Api(f"http://127.0.0.1:{PORT}")
    try:
        run(api, fx)
    finally:
        if not a.keep:
            sh("docker", "rm", "-f", NAME)
    print(f"\n{len(passed)} OK, {len(failed)} KO")
    sys.exit(1 if failed else 0)


def make_tv(work, image):
    """Live TV di prova: un canale MPEG-TS (H.264 + AAC, 30 s), una lista M3U e una guida XMLTV attorno ad adesso."""
    tv = os.path.join(work, "tv")
    r = sh("docker", "run", "--rm", "-u", "0", "-v", f"{tv}:/w", "--entrypoint", "/usr/lib/jellyfin-ffmpeg/ffmpeg", image, "-v", "error", "-y",
           "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=25", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
           "-t", "30", "-c:v", "libx264", "-preset", "veryfast", "-g", "50", "-c:a", "aac", "-f", "mpegts", "/w/ch1.ts")
    if r.returncode != 0:
        sys.exit(r.stderr)
    with open(os.path.join(tv, "list.m3u"), "w") as f:
        f.write('#EXTM3U\n#EXTINF:-1 tvg-id="prova.1" tvg-chno="1" group-title="Prova",Canale di prova\n/media/tv/ch1.ts\n')
    now = int(time.time()) // 1800 * 1800
    fmt = lambda t: time.strftime("%Y%m%d%H%M%S +0000", time.gmtime(t))
    progs = "".join(f'<programme start="{fmt(now + i * 1800)}" stop="{fmt(now + (i + 1) * 1800)}" channel="prova.1"><title>Programma {i}</title><desc>Prova</desc></programme>' for i in range(-2, 8))
    with open(os.path.join(tv, "epg.xml"), "w") as f:
        f.write(f'<?xml version="1.0"?><tv><channel id="prova.1"><display-name>Canale di prova</display-name></channel>{progs}</tv>')
    for n in os.listdir(tv):
        os.chmod(os.path.join(tv, n), 0o644)


def redact(o):
    if isinstance(o, dict):
        return {k: ("<token>" if k in ("token", "secret", "stream_token") else redact(v)) for k, v in o.items()}
    if isinstance(o, list):
        return [redact(v) for v in o]
    if isinstance(o, str):
        return re.sub(r"([?&](?:t|token|sig|key)=)[^&]+", r"\1x", o)
    return o


def save(fx, name, obj):
    obj = redact(obj)
    with open(os.path.join(fx, name + ".json"), "w") as f:
        json.dump(obj, f, ensure_ascii=False, indent=1)


def run(api, fx):
    wait(lambda: api.req("GET", "/api/v1/system/info")[0] == 200, 60, "avvio del server")
    code = wait(lambda: next((l.split("guidata:")[1].strip() for l in (sh("docker", "logs", NAME).stdout + sh("docker", "logs", NAME).stderr).splitlines() if "guidata:" in l), None), 60, "codice")
    st, r = api.req("POST", "/api/v1/setup/admin", {"code": code, "username": "admin", "password": "password-di-prova", "device_id": "android-test", "device_name": "Pixel di prova", "client": "Platea Android", "client_version": "test"})
    assert st == 200, r
    api.token = r["token"]
    save(fx, "login", r)
    libs = {}
    for name, kind, path in (("Film", "movies", "/media/Film"), ("Serie", "shows", "/media/Serie")):
        st, r = api.req("POST", "/api/v1/libraries", {"name": name, "kind": kind, "paths": [path], "scan_now": True})
        assert st == 200, r
        libs[kind] = r["id"]

    def ready():
        _, o = api.req("GET", "/api/v1/admin/overview")
        return o["library"]["files"] >= 5 and o["jobs"]["queued"] == 0 and o["jobs"]["running"] == 0
    wait(ready, 120, "scansione e analisi")
    time.sleep(1)

    def get(name, path, expect=200):
        st, r = api.req("GET", path)
        check(f"GET {path}", st == expect, f"{st} {r}")
        if st == 200:
            save(fx, name, r)
        return r

    get("system_info", "/api/v1/system/info")
    get("me", "/api/v1/me")
    get("preferences", "/api/v1/me/preferences")
    get("devices", "/api/v1/me/devices")
    get("libraries", "/api/v1/libraries")
    get("library", f"/api/v1/libraries/{libs['movies']}")
    get("genres", f"/api/v1/genres?library_id={libs['movies']}")
    movies = get("items_movies", f"/api/v1/items?library_id={libs['movies']}&sort=title&order=asc&limit=60")
    get("items_shows", f"/api/v1/items?library_id={libs['shows']}&sort=added&order=desc&limit=60&played=false")
    get("items_letter", f"/api/v1/items?library_id={libs['movies']}&sort=title&order=asc&limit=60&letter=P")
    get("search", "/api/v1/search?q=prova&limit=100")
    check("ricerca trova film e serie", len(json.load(open(os.path.join(fx, "search.json")))["items"]) >= 3)
    shows = json.load(open(os.path.join(fx, "items_shows.json")))["items"]
    series = shows[0]
    get("series", f"/api/v1/items/{series['id']}")
    seasons = get("seasons", f"/api/v1/items/{series['id']}/children")["items"]
    eps = get("episodes", f"/api/v1/items/{seasons[0]['id']}/children")["items"]
    get("missing", f"/api/v1/items/{seasons[0]['id']}/missing")
    ep = eps[0]
    get("episode", f"/api/v1/items/{ep['id']}")
    nxt = get("next", f"/api/v1/items/{ep['id']}/next")
    check("episodio successivo", nxt.get("item") and nxt["item"]["id"] == eps[1]["id"], json.dumps(nxt)[:200])
    by_title = {m["title"]: m for m in movies["items"]}
    for m in movies["items"]:
        slug = m["title"].lower().replace(" ", "_")
        get(f"movie_{slug}", f"/api/v1/items/{m['id']}")
        get(f"chapters_{slug}", f"/api/v1/items/{m['id']}/chapters")
        get(f"similar_{slug}", f"/api/v1/items/{m['id']}/similar")
    get("home_before", "/api/v1/home")

    # Riproduzione: decisioni con i profili tipici e ciclo completo (avanzamento, tracce, stop).
    def info(item, profile, **kw):
        body = {"item_id": item["id"], "profile": profile, "start_ms": 0, "ui_language": "it", "explicit_quality": False}
        body.update(kw)
        st, r = api.req("POST", "/api/v1/playback/info", body)
        check(f"playback/info {item['title']} ({profile['name']})", st == 200, f"{st} {r}")
        return r

    results = {}
    for title, m in sorted(by_title.items()):
        for prof in (PHONE, FIRE_OLD):
            p = info(m, prof)
            results[(title, prof["name"])] = p
            save(fx, f"playback_{title.lower().replace(' ', '_')}_{'tv' if prof is FIRE_OLD else 'phone'}", p)
            url = p.get("hls_url") or p.get("stream_url")
            st, body = api.req("GET", url)
            check(f"  flusso {title} {prof['name']} {p['method']}", st in (200, 206) and len(body) > 0, f"{st}")
            for s in p["subtitles"]:
                if s.get("url"):
                    st2, vtt = api.req("GET", s["url"])
                    check(f"  sottotitolo {s['index']} WebVTT", st2 == 200 and vtt[:6] == b"WEBVTT", f"{st2} {vtt[:20]}")
            api.req("POST", f"/api/v1/playback/{p['session_id']}/stop", {"position_ms": 0})
    for k, p in results.items():
        print(f"     {k[0]:<22} {k[1]:<20} {p['method']:<12} {p['chain']}  {p['reasons']}")
    check("telefono: HEVC copiato (niente transcodifica)", results[("Prova HEVC", "Platea Android")]["method"] in ("direct_play", "remux"))
    check("telefono: MP4 in direct play", results[("Prova Diretta", "Platea Android")]["method"] == "direct_play")
    check("Fire TV vecchio: HEVC transcodificato", results[("Prova HEVC", "Platea Android TV")]["method"] == "transcode")

    # Episodio con sottotitolo esterno .srt → WebVTT a parte.
    p = info(ep, PHONE)
    save(fx, "playback_episode_phone", p)
    check("episodio: sottotitolo esterno con url", any(s.get("url") for s in p["subtitles"]), json.dumps(p["subtitles"])[:200])
    sid = p["session_id"]
    st, _ = api.req("POST", f"/api/v1/playback/{sid}/progress", {"position_ms": 4000, "paused": False})
    check("progress", st in (200, 204), str(st))
    st, _ = api.req("POST", f"/api/v1/playback/{sid}/tracks", {"audio_index": p.get("audio_index"), "subtitle_index": None, "subtitle_off": True})
    check("tracks", st in (200, 204), str(st))
    st, _ = api.req("POST", f"/api/v1/playback/{sid}/client-error", {"message": "prova dall'app Android"})
    check("client-error", st in (200, 204), str(st))
    st, _ = api.req("POST", f"/api/v1/playback/{sid}/stop", {"position_ms": 5000})
    check("stop", st in (200, 204), str(st))
    # I file di prova durano 20 s: sotto la soglia minima di ripresa il server non salva la posizione.
    get("home", "/api/v1/home")

    # Visto, preferito.
    m = by_title["Prova Diretta"]
    for what in ("played", "favorite"):
        st, _ = api.req("POST", f"/api/v1/items/{m['id']}/{what}")
        check(f"POST {what}", st in (200, 204), str(st))
    d = get("movie_after", f"/api/v1/items/{m['id']}")
    check("visto e preferito salvati", d["user_data"]["played"] and d["user_data"]["favorite"])
    for what in ("played", "favorite"):
        api.req("DELETE", f"/api/v1/items/{m['id']}/{what}")

    # Playlist: crea, aggiungi, sposta, condividi, togli, cancella.
    st, r = api.req("POST", "/api/v1/playlists", {"name": "Prova", "item_ids": [m["id"]]})
    check("crea playlist", st == 200, f"{st} {r}")
    pid = r["id"]
    st, r = api.req("POST", f"/api/v1/playlists/{pid}/items", {"item_ids": [series["id"]]})
    check("aggiungi serie (si espande negli episodi)", st == 200, f"{st} {r}")
    pl = get("playlist", f"/api/v1/playlists/{pid}")
    check("playlist con 3 voci", len(pl["items"]) == 3, str(len(pl["items"])))
    st, _ = api.req("POST", f"/api/v1/playlists/{pid}/move", {"entry_id": pl["items"][2]["entry_id"], "index": 0})
    check("sposta voce", st in (200, 204), str(st))
    st, _ = api.req("PATCH", f"/api/v1/playlists/{pid}", {"shared": True, "name": "Prova 2"})
    check("rinomina e condividi", st in (200, 204), str(st))
    get("playlists", "/api/v1/playlists")
    st, _ = api.req("DELETE", f"/api/v1/playlists/{pid}/items/{pl['items'][0]['entry_id']}")
    check("togli voce", st in (200, 204), str(st))
    st, _ = api.req("DELETE", f"/api/v1/playlists/{pid}")
    check("cancella playlist", st in (200, 204), str(st))

    # Live TV: sorgente M3U + XMLTV locale, stato, canali con il programma in onda, avvio di un canale.
    st, r = api.req("POST", "/api/v1/admin/livetv/sources", {"kind": "m3u", "name": "Prova", "url": "/media/tv/list.m3u", "epg_url": "/media/tv/epg.xml"})
    check("sorgente TV", st == 200, f"{st} {r}")
    wait(lambda: any(c.get("now") for c in (api.req("GET", "/api/v1/livetv/channels")[1] or {}).get("items", [])), 60, "canali e guida TV")
    get("livetv_status", "/api/v1/livetv/status")
    ch = get("livetv_channels", "/api/v1/livetv/channels")
    check("canale con il programma in onda", bool(ch["items"]) and ch["items"][0].get("now") is not None, json.dumps(ch)[:300])
    st, lp = api.req("POST", f"/api/v1/livetv/channels/{ch['items'][0]['id']}/play", {"profile": PHONE})
    check("avvio di un canale (HLS dal vivo)", st == 200 and lp.get("hls_url"), f"{st} {lp}")
    if st == 200:
        save(fx, "livetv_play", lp)
        st, _ = api.req("POST", f"/api/v1/livetv/sessions/{lp['session_id']}/stop")
        check("fine della visione", st in (200, 204), str(st))

    # Preferenze: l'app rimanda l'oggetto intero.
    _, prefs = api.req("GET", "/api/v1/me/preferences")
    prefs["skip_forward_seconds"] = 15
    st, r = api.req("PUT", "/api/v1/me/preferences", prefs)
    check("salva preferenze", st == 200 and r["skip_forward_seconds"] == 15, f"{st} {r}")

    # Quick Connect: la TV chiede un codice, il telefono lo conferma, la TV riceve il token.
    tv = Api(api.base)
    st, r = tv.req("POST", "/api/v1/quickconnect/initiate", {"device_id": "tv-test", "device_name": "Fire TV di prova", "client": "Platea Android TV", "client_version": "test"})
    if check("quick connect: codice", st == 200, f"{st} {r}"):
        save(fx, "qc_initiate", r)
        st, s = tv.req("GET", f"/api/v1/quickconnect/status/{r['secret']}")
        save(fx, "qc_status", s)
        st, _ = api.req("POST", "/api/v1/quickconnect/authorize", {"code": r["code"]})
        check("quick connect: conferma", st in (200, 204), str(st))
        st, t = tv.req("POST", "/api/v1/quickconnect/redeem", {"secret": r["secret"]})
        check("quick connect: token", st == 200 and t.get("token"), f"{st} {t}")
        save(fx, "qc_redeem", t)

    # Accesso dell'app (stesso corpo di Session.login) e uscita.
    app = Api(api.base)
    st, r = app.req("POST", "/api/v1/auth/login", {"username": "admin", "password": "password-di-prova", "device_id": "app-1", "device_name": "Pixel 9", "client": "Platea Android", "client_version": "0.1.6"})
    check("login dell'app", st == 200, f"{st} {r}")
    app.token = r["token"]
    st, _ = app.req("POST", "/api/v1/auth/logout")
    check("logout", st in (200, 204), str(st))
    st, _ = app.req("GET", "/api/v1/me")
    check("token revocato → 401", st == 401, str(st))
    st, r = api.req("POST", "/api/v1/auth/login", {"username": "admin", "password": "sbagliata", "device_id": "x"})
    check("password sbagliata → messaggio", st in (400, 401, 403) and isinstance(r, dict) and r.get("message"), f"{st} {r}")
    save(fx, "error_login", r)


if __name__ == "__main__":
    main()
