#!/usr/bin/env python3
"""Prova end-to-end dei nodi: un server e due nodi in container separati, file video sintetici.

    scripts/nodes-test/run.sh            # costruisce l'immagine e lancia tutto
    python3 scripts/nodes-test/nodes_test.py --image platea-server:nodes-dev --work /tmp/nt

Mai i media dell'utente: i file si generano nella cartella di lavoro. Nodo 1 senza copie (riceve i file dal
server), nodo 2 con la libreria in copia (CDN e transcodifica dal file locale).
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request

NET = "platea-nt"
SRV, N1, N2 = "platea-nt-srv", "platea-nt-n1", "platea-nt-n2"
SRV_PORT, N2_PORT = 18096, 18098
KEY1, KEY2 = "chiave-del-nodo-uno-0123456789", "chiave-del-nodo-due-0123456789"
FFMPEG = "/usr/lib/jellyfin-ffmpeg/ffmpeg"

passed, failed = [], []


def sh(*args, check=True, capture=True):
    r = subprocess.run(list(args), capture_output=capture, text=True)
    if check and r.returncode != 0:
        raise RuntimeError(f"{' '.join(args)}\n{r.stdout}\n{r.stderr}")
    return r.stdout if capture else ""


def check(name, cond, detail=""):
    (passed if cond else failed).append(name)
    print(("  OK  " if cond else "  KO  ") + name + (f"  [{detail}]" if detail and not cond else ""), flush=True)
    return cond


class Api:
    def __init__(self, base):
        self.base = base
        self.token = None

    def req(self, method, path, body=None, headers=None, raw=False, token=None, follow=True):
        h = {"Accept": "application/json"}
        tok = token or self.token
        if tok:
            h["Authorization"] = f"Bearer {tok}"
        data = None
        if body is not None:
            h["Content-Type"] = "application/json"
            data = json.dumps(body).encode()
        h.update(headers or {})
        r = urllib.request.Request(self.base + path, data=data, method=method, headers=h)
        opener = urllib.request.build_opener() if follow else urllib.request.build_opener(NoRedirect)
        try:
            with opener.open(r, timeout=120) as resp:
                content = resp.read()
                return resp.status, (content if raw else (json.loads(content) if content else None)), {k.lower(): v for k, v in resp.headers.items()}
        except urllib.error.HTTPError as e:
            content = e.read()
            try:
                payload = json.loads(content) if content and not raw else content
            except ValueError:
                payload = content
            return e.code, payload, {k.lower(): v for k, v in e.headers.items()}

    def get(self, path, **kw):
        return self.req("GET", path, **kw)

    def post(self, path, body=None, **kw):
        return self.req("POST", path, body if body is not None else {}, **kw)


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *a, **k):
        return None


def wait(cond, timeout, every=1.0, what="condizione"):
    t0 = time.time()
    while time.time() - t0 < timeout:
        try:
            v = cond()
            if v:
                return v
        except Exception:  # noqa: BLE001 - si riprova
            pass
        time.sleep(every)
    raise TimeoutError(f"tempo scaduto: {what}")


def logs_of(c):
    r = subprocess.run(["docker", "logs", c], capture_output=True, text=True)
    return r.stdout + r.stderr


def cleanup():
    for c in (SRV, N1, N2):
        subprocess.run(["docker", "rm", "-f", c], capture_output=True)
    subprocess.run(["docker", "network", "rm", NET], capture_output=True)


def make_media(image, work):
    film = os.path.join(work, "media", "film")
    if os.path.exists(os.path.join(film, "Due (2021)", "Due (2021).mkv")):
        return
    os.makedirs(os.path.join(film, "Uno (2020)"), exist_ok=True)
    os.makedirs(os.path.join(film, "Due (2021)"), exist_ok=True)
    run = ["docker", "run", "--rm", "--user", "0", "--entrypoint", FFMPEG, "-v", f"{work}/media:/m", image, "-hide_banner", "-loglevel", "error", "-y"]
    # 1080p H.264 + AAC, keyframe ogni 2 s, 40 s.
    sh(*run, "-f", "lavfi", "-i", "testsrc2=size=1920x1080:rate=25", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
       "-t", "40", "-c:v", "libx264", "-preset", "ultrafast", "-g", "50", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "128k",
       "/m/film/Uno (2020)/Uno (2020).mkv")
    # 4K HEVC 10 bit con metadati HDR10, 16 s.
    sh(*run, "-f", "lavfi", "-i", "testsrc2=size=3840x2160:rate=24", "-f", "lavfi", "-i", "sine=frequency=220:sample_rate=48000",
       "-t", "16", "-c:v", "libx265", "-preset", "ultrafast", "-pix_fmt", "yuv420p10le",
       "-x265-params", "log-level=error:keyint=48:colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc:hdr10=1",
       "-color_primaries", "bt2020", "-color_trc", "smpte2084", "-colorspace", "bt2020nc", "-c:a", "aac",
       "/m/film/Due (2021)/Due (2021).mkv")


def start(image, work):
    cleanup()
    for d in ("srv-data", "srv-cache", "srv-transcode", "n1-data", "n2-data"):
        p = os.path.join(work, d)
        shutil.rmtree(p, ignore_errors=True)
        os.makedirs(p)
    copia = os.path.join(work, "copia")
    shutil.rmtree(copia, ignore_errors=True)
    shutil.copytree(os.path.join(work, "media"), copia)
    sh("docker", "run", "--rm", "--user", "0", "--entrypoint", "chown", "-v", f"{work}:/w", image, "-R", "1000:1000",
       "/w/srv-data", "/w/srv-cache", "/w/srv-transcode", "/w/n1-data", "/w/n2-data")
    # Sottorete esplicita: quella scelta da Docker può cadere su una LAN raggiunta via VPN (es. 192.168.0.0/24 da Tailscale).
    sh("docker", "network", "create", "--subnet", os.environ.get("PLATEA_NT_SUBNET", "10.201.77.0/24"), NET)
    sh("docker", "run", "-d", "--name", SRV, "--network", NET, "-p", f"127.0.0.1:{SRV_PORT}:8096",
       "-v", f"{work}/srv-data:/data", "-v", f"{work}/srv-cache:/cache", "-v", f"{work}/srv-transcode:/transcode",
       "-v", f"{work}/media:/media:ro", "-e", "PLATEA_LOG=info", image)
    node = ["docker", "run", "-d", "--network", NET, "-e", "PLATEA_ROLE=node", "-e", "PLATEA_LOG=info,platea=debug"]
    sh(*node, "--name", N1, "-e", f"PLATEA_NODE_KEY={KEY1}", "-e", "PLATEA_NODE_NAME=uno", "--tmpfs", "/transcode:uid=1000,gid=1000",
       "-v", f"{work}/n1-data:/data", image, "node")
    sh(*node, "--name", N2, "-p", f"127.0.0.1:{N2_PORT}:8097", "-e", f"PLATEA_NODE_KEY={KEY2}", "-e", "PLATEA_NODE_NAME=due",
       "--tmpfs", "/transcode:uid=1000,gid=1000", "-v", f"{work}/n2-data:/data", "-v", f"{copia}:/media/copia:ro", image, "node")


def setup(api):
    code = wait(lambda: next((l.split("guidata:")[1].strip() for l in logs_of(SRV).splitlines() if "guidata:" in l), None), 60, what="codice di primo avvio")
    st, r, _ = api.post("/api/v1/setup/admin", {"code": code, "username": "admin", "password": "password-di-prova", "device_id": "nt-0", "device_name": "prova"})
    assert st == 200, r
    api.token = r["token"]
    st, r, _ = api.post("/api/v1/libraries", {"name": "Film", "kind": "movies", "paths": ["/media/film"], "scan_now": True})
    assert st == 200, r
    lib = r["id"]

    def probed():
        _, o, _ = api.get("/api/v1/admin/overview")
        _, j, _ = api.get("/api/v1/admin/jobs?state=queued")
        return o["library"]["files"] == 2 and o["jobs"]["queued"] == 0 and o["jobs"]["running"] == 0

    wait(probed, 120, what="scansione e analisi")
    _, items, _ = api.get(f"/api/v1/items?library_id={lib}&limit=10")
    ids = {i["title"]: i["id"] for i in items["items"]}
    return ids


def login(api, n):
    st, r, _ = api.post("/api/v1/auth/login", {"username": "admin", "password": "password-di-prova", "device_id": f"nt-{n}", "device_name": f"prova {n}"}, token="-")
    assert st == 200, r
    return r["token"]


H264_PROFILE = {
    "name": "prova", "containers": ["mp4"], "protocols": ["hls"], "hls_video_codecs": ["h264"], "hls_audio_codecs": ["aac"],
    "video_codecs": [{"codec": "h264", "max_bit_depth": 8}], "audio_codecs": [{"codec": "aac", "max_channels": 2}],
}
DIRECT_PROFILE = {
    "name": "prova", "containers": ["mkv", "mp4"], "protocols": ["file", "hls"], "hls_video_codecs": ["h264"], "hls_audio_codecs": ["aac"],
    "video_codecs": [{"codec": "h264", "max_bit_depth": 8}], "audio_codecs": [{"codec": "aac", "max_channels": 2}],
}


def play(api, token, item, force=True, profile=None):
    st, r, _ = api.post("/api/v1/playback/info", {"item_id": item, "profile": profile or H264_PROFILE, "force_transcode": force}, token=token)
    assert st == 200, r
    return r


def sessions(api):
    return {s["id"]: s for s in api.get("/api/v1/admin/sessions")[1]["items"]}


def segment(api, info, n):
    sid = info["session_id"]
    return api.get(f"/api/v1/hls/{sid}/{n}.ts?t={info['stream_token']}", raw=True)


def node_by_name(api, name):
    _, d, _ = api.get("/api/v1/admin/nodes")
    return next(n for n in d["nodes"] if (n["name"] == name) or (name == "local" and n["is_local"]))


def patch_node(api, name, body):
    n = node_by_name(api, name)
    st, r, _ = api.req("PATCH", f"/api/v1/admin/nodes/{n['id']}", body)
    assert st == 200, r
    return r


def stop_all(api, infos):
    for i in infos:
        api.post(f"/api/v1/playback/{i['session_id']}/stop", {"position_ms": 0})


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--image", default="platea-server:nodes-dev")
    ap.add_argument("--work", required=True)
    ap.add_argument("--keep", action="store_true", help="lascia i container accesi alla fine")
    ap.add_argument("--skip-orphan", action="store_true", help="salta la prova del server spento (60 s)")
    a = ap.parse_args()
    work = os.path.abspath(a.work)
    os.makedirs(work, exist_ok=True)
    print("== file di prova", flush=True)
    make_media(a.image, work)
    print("== container", flush=True)
    start(a.image, work)
    api = Api(f"http://127.0.0.1:{SRV_PORT}")
    wait(lambda: api.get("/api/v1/health")[0] == 200, 60, what="server pronto")
    ids = setup(api)
    uno, due = ids["Uno"], ids["Due"]
    print(f"== libreria pronta: {ids}", flush=True)

    # --- registro e stato
    print("== nodi", flush=True)
    st, r, _ = api.post("/api/v1/admin/nodes", {"name": "uno", "url": f"{N1}:8097", "key": KEY1, "role_transcode": True})
    check("aggiunta del nodo uno", st == 200 and r["url"] == f"http://{N1}:8097", r)
    st, r, _ = api.post("/api/v1/admin/nodes", {"name": "due", "url": f"http://{N2}:8097", "key": KEY2, "role_transcode": True, "role_cdn": True})
    check("aggiunta del nodo due", st == 200, r)
    st, r, _ = api.post("/api/v1/admin/nodes", {"name": "uno-bis", "url": f"{N1}:8097", "key": KEY2})
    check("indirizzo duplicato rifiutato", st == 409, r)
    st, r, _ = api.post("/api/v1/admin/nodes", {"name": "sbagliata", "url": f"{N1}:8097/", "key": "x" * 20})
    check("indirizzo duplicato rifiutato anche con la barra", st == 409, r)
    wait(lambda: node_by_name(api, "uno")["health"] == "online" and node_by_name(api, "due")["health"] == "online", 60, what="nodi online")
    n1 = node_by_name(api, "uno")
    check("stato del nodo con capacità", n1["status"]["ffmpeg_available"] and "h264" in n1["status"]["encoders"], n1["status"])
    check("configurazione spinta al nodo", wait(lambda: node_by_name(api, "uno")["config_synced"], 20, what="config"), "")
    check("indirizzo del server ricavato dal nodo", (n1.get("server_url") or "").startswith("http://") and n1["server_url"].endswith(":8096"), n1.get("server_url"))
    # chiave sbagliata
    bad = sh("docker", "run", "--rm", "--network", NET, "curlimages/curl:8.11.1", "-s", "-o", "/dev/null", "-w", "%{http_code}",
             "-H", "Authorization: Bearer sbagliata", f"http://{N1}:8097/node/v1/status", check=False).strip()
    check("il nodo rifiuta una chiave sbagliata", bad == "401", bad)
    health = sh("docker", "run", "--rm", "--network", NET, "curlimages/curl:8.11.1", "-s", f"http://{N1}:8097/node/v1/health", check=False)
    check("salute del nodo senza chiave", '"ok":true' in health, health)
    lp = api.get("/api/v1/admin/nodes")[1]["library_paths"][0]["id"]
    st, r, _ = api.req("PUT", f"/api/v1/admin/nodes/{node_by_name(api, 'due')['id']}/paths", {"paths": [{"library_path_id": lp, "node_path": "/media/copia/film"}]})
    check("cartella in copia verificata", st == 200 and r["results"][0]["ok"], r)
    st, r, _ = api.req("PUT", f"/api/v1/admin/nodes/{node_by_name(api, 'uno')['id']}/paths", {"paths": [{"library_path_id": lp, "node_path": "/etc"}]})
    check("cartella fuori dalle radici del nodo segnalata", st == 200 and not r["results"][0]["ok"], r)
    api.req("PUT", f"/api/v1/admin/nodes/{node_by_name(api, 'uno')['id']}/paths", {"paths": []})
    st, r, _ = api.get(f"/api/v1/admin/nodes/{node_by_name(api, 'due')['id']}/browse?path=/media/copia")
    check("sfoglia le cartelle del nodo", st == 200 and any(d["name"] == "film" for d in r["directories"]), r)
    st, r, _ = api.get(f"/api/v1/admin/nodes/{node_by_name(api, 'due')['id']}/browse?path=/etc")
    check("sfoglia fuori dalle radici rifiutato", st != 200, r)

    # --- prova di velocità
    print("== prova di velocità", flush=True)
    nid1 = node_by_name(api, "uno")["id"]
    t0 = time.time()
    st, r, _ = api.post(f"/api/v1/admin/nodes/{nid1}/speed-test")
    took = time.time() - t0
    sp = (r or {}).get("result") or {}
    ok_dir = lambda d: bool(d) and d.get("error") is None and (d.get("mbps") or 0) > 1 and d.get("bytes", 0) > 0
    print(f"   velocità: {sp.get('to_node', {}).get('mbps')} / {sp.get('from_node', {}).get('mbps')} / {(sp.get('node_reads') or {}).get('mbps')} Mbit/s, latenza {sp.get('latency_ms')} ms, {took:.1f} s", flush=True)
    check("velocità server → nodo", st == 200 and ok_dir(sp.get("to_node")), sp.get("to_node"))
    check("velocità nodo → server", st == 200 and ok_dir(sp.get("from_node")), sp.get("from_node"))
    check("velocità del nodo che legge dal server", st == 200 and ok_dir(sp.get("node_reads")) and sp.get("server_url", "").endswith(":8096"), sp.get("node_reads"))
    check(f"prova di velocità in tempo ({took:.1f} s)", took < 30 and sp.get("latency_ms") is not None, took)
    check("ultima prova nella scheda del nodo", (node_by_name(api, "uno").get("speed") or {}).get("at") == sp.get("at"), node_by_name(api, "uno").get("speed"))
    st, _, _ = api.get("/api/v1/node-speed/inventato")
    check("dati della prova solo con token", st == 401, st)
    st, r, _ = api.post(f"/api/v1/admin/nodes/{node_by_name(api, 'local')['id']}/speed-test")
    check("niente prova di velocità su questo server", st != 200, (st, r))

    # --- pesi: server di riserva (priorità 2), uno 90, due 10, senza preferenza per la copia locale
    print("== bilanciamento", flush=True)
    api.req("PUT", "/api/v1/admin/settings/nodes", {"prefer_local_copy": False})
    patch_node(api, "local", {"tc_priority": 2})
    patch_node(api, "uno", {"tc_weight": 90})
    patch_node(api, "due", {"tc_weight": 10})
    infos = []
    for k in range(10):
        infos.append(play(api, login(api, 100 + k), uno))
    ex = [sessions(api)[i["session_id"]].get("executor") for i in infos]
    check("pesi 90/10: 9 sessioni su uno, 1 su due", ex.count("uno") == 9 and ex.count("due") == 1, ex)
    st, body, _ = segment(api, infos[0], 0)
    check("segmento transcodificato dal nodo", st == 200 and body[:1] == b"\x47", (st, body[:80]))
    chain = sessions(api)[infos[0]["session_id"]]["chain"]
    check("riga tecnica con il nodo", chain.endswith("· uno"), chain)
    logs_n1 = logs_of(N1)
    check("il nodo senza copia legge dal server", "node-source" in logs_n1, logs_n1[-800:])
    stop_all(api, infos)

    # --- lettura dal server a blocchi (il nodo uno non ha copie)
    print("== lettura dal server a blocchi", flush=True)
    check("predefinita: lettura anticipata con cache", "lettura anticipata" in logs_n1, logs_n1[-800:])
    cfg = node_by_name(api, "uno")["config"]
    check("impostazioni predefinite della lettura", cfg.get("source_mode") == "readahead" and cfg.get("readahead_mb") == 1024, cfg)
    sr = play(api, login(api, 120), uno)
    st, body, _ = segment(api, sr, 4)
    ex = sessions(api)[sr["session_id"]].get("executor")
    check("salto avanti con la lettura anticipata, sempre sul nodo", st == 200 and body[:1] == b"\x47" and ex == "uno", (st, ex))
    stop_all(api, [sr])
    stn = node_by_name(api, "uno")["status"]
    check("il file resta nella cache del nodo", stn.get("source_cache_files", 0) >= 1 and stn.get("source_cache_bytes", 0) > 0, stn.get("source_cache_files"))
    patch_node(api, "uno", {"config": {**cfg, "source_mode": "full"}})
    wait(lambda: node_by_name(api, "uno")["config_synced"], 20, what="config full")
    fc = play(api, login(api, 121), due)
    check("copia completa sul nodo uno", sessions(api)[fc["session_id"]].get("executor") == "uno", sessions(api)[fc["session_id"]].get("executor"))
    st, body, _ = segment(api, fc, 0)
    ex = sessions(api)[fc["session_id"]].get("executor")
    check("segmento con la copia completa, sempre sul nodo", st == 200 and body[:1] == b"\x47" and ex == "uno", (st, ex))
    try:
        full_done = wait(lambda: "copia completa" in logs_of(N1) and "complete=true" in logs_of(N1), 60, what="copia completa finita")
    except TimeoutError:
        full_done = False
    check("copia completa scaricata tutta", full_done, logs_of(N1)[-800:])
    stop_all(api, [fc])
    patch_node(api, "uno", {"config": {**cfg, "source_mode": "stream"}})
    wait(lambda: node_by_name(api, "uno")["config_synced"], 20, what="config stream")
    ds = play(api, login(api, 122), uno)
    st, body, _ = segment(api, ds, 0)
    check("lettura in diretta (come prima)", st == 200 and body[:1] == b"\x47" and 'via="diretta"' in logs_of(N1), (st, body[:80]))
    stop_all(api, [ds])
    st, r, _ = api.req("PATCH", f"/api/v1/admin/nodes/{node_by_name(api, 'uno')['id']}", {"config": {**cfg, "source_parallel": 99}})
    check("impostazioni della lettura fuori dai limiti rifiutate", st == 400, r)
    patch_node(api, "uno", {"config": cfg})
    wait(lambda: node_by_name(api, "uno")["config_synced"], 20, what="config readahead")

    # --- regole: uno solo 4K, due solo fino a 1080p
    print("== regole", flush=True)
    patch_node(api, "uno", {"rules": {"min_height": 2160}})
    patch_node(api, "due", {"rules": {"max_height": 1080}})
    a1 = play(api, login(api, 200), due)
    a2 = play(api, login(api, 201), uno)
    s = sessions(api)
    check("4K va al nodo solo-4K", s[a1["session_id"]].get("executor") == "uno", s[a1["session_id"]].get("executor"))
    check("1080p va al nodo fino-a-1080p", s[a2["session_id"]].get("executor") == "due", s[a2["session_id"]].get("executor"))
    st, body, _ = segment(api, a1, 0)
    check("segmento 4K HDR transcodificato (tone mapping) dal nodo", st == 200 and body[:1] == b"\x47", (st, body[:80]))
    st, body, _ = segment(api, a2, 0)
    check("segmento dal nodo con la copia", st == 200 and body[:1] == b"\x47", (st, body[:80]))
    logs_n2 = logs_of(N2)
    check("1+2: il nodo con la copia transcodifica dal file locale", 'Path("/media/copia/film' in logs_n2, logs_n2[-800:])
    stop_all(api, [a1, a2])
    patch_node(api, "uno", {"rules": {}})
    patch_node(api, "due", {"rules": {}})

    # --- preferenza per chi ha il file
    api.req("PUT", "/api/v1/admin/settings/nodes", {"prefer_local_copy": True})
    b = play(api, login(api, 210), uno)
    check("a parità di priorità vince chi ha la copia", sessions(api)[b["session_id"]].get("executor") == "due", sessions(api)[b["session_id"]].get("executor"))
    stop_all(api, [b])

    # --- il nodo non raggiunge il server
    print("== nodo che non raggiunge il server", flush=True)
    cfg1 = node_by_name(api, "uno")["config"]
    api.req("PUT", "/api/v1/admin/settings/nodes", {"server_url": f"http://{SRV}:9"})
    patch_node(api, "due", {"draining": True})
    st, r, _ = api.post(f"/api/v1/admin/nodes/{node_by_name(api, 'uno')['id']}/check")
    reach = r.get("server_reach") or {}
    check("prova: il nodo non raggiunge un indirizzo sbagliato", st == 200 and reach.get("ok") is False and reach.get("error"), reach)
    check("il server lo segnala nei log", "il nodo non raggiunge il server" in logs_of(SRV), "")
    st, r, _ = api.post(f"/api/v1/admin/nodes/{node_by_name(api, 'uno')['id']}/speed-test")
    nr = ((r or {}).get("result") or {}).get("node_reads") or {}
    check("prova di velocità: lettura dal server a un indirizzo sbagliato segnalata", st == 200 and nr.get("error") and ((r["result"].get("from_node") or {}).get("mbps") or 0) > 0, r and r.get("result"))
    # "Il nodo legge dal server": escluso subito invece di fallire dopo l'avvio.
    patch_node(api, "uno", {"config": {**cfg1, "source_transport": "pull"}})
    wait(lambda: node_by_name(api, "uno")["config_synced"], 20, what="config pull")
    t0 = time.time()
    c = play(api, login(api, 212), uno)
    ex = sessions(api)[c["session_id"]].get("executor")
    check("in pull, nodo che non raggiunge il server escluso: transcodifica il server", ex is None, ex)
    st, body, _ = segment(api, c, 0)
    check(f"primo segmento senza attesa di failover ({time.time() - t0:.1f} s)", st == 200 and body[:1] == b"\x47" and time.time() - t0 < 15, (st, time.time() - t0))
    stop_all(api, [c])
    # Automatico: il server gli manda il file. Cache svuotata prima, così i blocchi arrivano davvero dal server.
    patch_node(api, "uno", {"config": {**cfg1, "source_transport": "auto", "source_cache_gb": 0}})
    wait(lambda: node_by_name(api, "uno")["config_synced"], 20, what="config auto")
    try:
        wait(lambda: node_by_name(api, "uno")["status"].get("source_cache_files") == 0, 60, what="cache svuotata")
    except TimeoutError:
        pass
    check("cache del nodo svuotata prima del push", node_by_name(api, "uno")["status"].get("source_cache_files") == 0, node_by_name(api, "uno")["status"].get("source_cache_files"))
    c = play(api, login(api, 214), due)
    ex = sessions(api)[c["session_id"]].get("executor")
    check("in automatico il nodo che non raggiunge il server riceve il file dal server", ex == "uno", ex)
    st, body, _ = segment(api, c, 0)
    check("segmento con il file mandato dal server", st == 200 and body[:1] == b"\x47", (st, body[:80]))
    st, body, _ = segment(api, c, 2)
    ex = sessions(api)[c["session_id"]].get("executor")
    check("segmento successivo in push, sempre sul nodo", st == 200 and body[:1] == b"\x47" and ex == "uno", (st, ex))
    check("il nodo dice che il file arriva dal server", "mandato dal server" in logs_of(N1), logs_of(N1)[-600:])
    stop_all(api, [c])
    try:
        sent = wait(lambda: "invio del file al nodo finito" in logs_of(SRV), 30, what="fine invio")
    except TimeoutError:
        sent = False
    check("il server smette di mandare a sessione chiusa", sent, logs_of(SRV)[-600:])
    sent_lines = [l for l in logs_of(SRV).splitlines() if "invio del file al nodo finito" in l]
    check("il server ha mandato davvero dei dati", any("mb=0 " not in l for l in sent_lines), sent_lines)
    try:
        wait(lambda: node_by_name(api, "uno")["status"].get("source_cache_files") == 0, 60, what="cache svuotata 2")
    except TimeoutError:
        pass
    # "Il server manda" anche con l'indirizzo giusto, copia completa.
    api.req("PUT", "/api/v1/admin/settings/nodes", {"server_url": ""})
    patch_node(api, "uno", {"config": {**cfg1, "source_transport": "push", "source_mode": "full", "source_cache_gb": 0}})
    n_sent = len([l for l in logs_of(SRV).splitlines() if "invio del file al nodo finito" in l])
    wait(lambda: node_by_name(api, "uno")["config_synced"], 20, what="config push")
    c = play(api, login(api, 215), uno)
    ex = sessions(api)[c["session_id"]].get("executor")
    st, body, _ = segment(api, c, 5)
    ex2 = sessions(api)[c["session_id"]].get("executor")
    check("push forzato con copia completa e salto avanti", ex == "uno" and ex2 == "uno" and st == 200 and body[:1] == b"\x47", (ex, ex2, st))
    try:
        done = wait(lambda: "mandato dal server, copia completa" in logs_of(N1) and "complete=true" in logs_of(SRV), 60, what="push completo")
    except TimeoutError:
        done = False
    check("push: file mandato tutto", done, logs_of(SRV)[-600:])
    last = [l for l in logs_of(SRV).splitlines() if "invio del file al nodo finito" in l][n_sent:]
    check("push: copia completa mandata dal server (non dalla cache)", any("complete=true" in l and "mb=0 " not in l for l in last), last)
    stop_all(api, [c])
    try:
        emptied = wait(lambda: node_by_name(api, "uno")["status"].get("source_cache_files") == 0, 30, what="cache vuota")
    except TimeoutError:
        emptied = False
    check("cache a 0 GB: file tolto a fine visione", emptied, node_by_name(api, "uno")["status"].get("source_cache_files"))
    patch_node(api, "uno", {"config": cfg1})
    wait(lambda: node_by_name(api, "uno")["config_synced"], 20, what="config ripristinata")
    try:
        ok = wait(lambda: (node_by_name(api, "uno").get("server_reach") or {}).get("ok") is True, 30, what="prova di nuovo riuscita")
    except TimeoutError:
        ok = False
    check("indirizzo corretto: la prova torna verde da sola", ok, node_by_name(api, "uno").get("server_reach"))
    patch_node(api, "due", {"draining": False})

    # --- remux: solo su chi ha il file
    pr = dict(H264_PROFILE)
    r = play(api, login(api, 211), uno, force=False, profile=pr)
    s = sessions(api)[r["session_id"]]
    check("remux su un nodo solo se ha il file", r["method"] == "remux" and s.get("executor") in ("due", None), (r["method"], s.get("executor")))
    st, body, _ = segment(api, r, 0)
    check("segmento remux", st == 200 and body[:1] == b"\x47", (st, body[:80]))
    stop_all(api, [r])

    pre = logs_of(SRV)
    check("nessun failover prima della prova di failover", "failover" not in pre, [l for l in pre.splitlines() if "failover" in l][:3])

    # --- failover a metà sessione
    print("== failover", flush=True)
    api.req("PUT", "/api/v1/admin/settings/nodes", {"prefer_local_copy": False})
    patch_node(api, "uno", {"tc_weight": 100, "tc_priority": 1})
    patch_node(api, "due", {"tc_priority": 3})
    f = play(api, login(api, 300), uno)
    check("sessione partita sul nodo uno", sessions(api)[f["session_id"]].get("executor") == "uno", sessions(api)[f["session_id"]].get("executor"))
    st0, b0, _ = segment(api, f, 0)
    sh("docker", "kill", N1)
    t0 = time.time()
    st1, b1, _ = segment(api, f, 1)
    st2, b2, _ = segment(api, f, 2)
    dt = time.time() - t0
    after = sessions(api)[f["session_id"]].get("executor")
    check("dopo la caduta del nodo i segmenti continuano", st0 == 200 and st1 == 200 and st2 == 200 and b1[:1] == b"\x47", (st0, st1, st2, b1[:60]))
    check("la sessione passa al server (priorità 2)", after is None, after)
    check(f"failover rapido ({dt:.1f} s)", dt < 30, dt)
    stop_all(api, [f])
    wait(lambda: node_by_name(api, "uno")["health"] == "offline", 30, what="uno offline")
    g = play(api, login(api, 301), uno)
    check("nodo offline escluso dalle nuove sessioni", sessions(api)[g["session_id"]].get("executor") is None, sessions(api)[g["session_id"]].get("executor"))
    stop_all(api, [g])
    sh("docker", "start", N1)
    check("il nodo torna online da solo", bool(wait(lambda: node_by_name(api, "uno")["health"] == "online", 60, what="uno online")), "")

    # --- svuotamento
    patch_node(api, "uno", {"draining": True})
    h = play(api, login(api, 302), uno)
    check("nodo in svuotamento non riceve sessioni", sessions(api)[h["session_id"]].get("executor") != "uno", sessions(api)[h["session_id"]].get("executor"))
    stop_all(api, [h])
    patch_node(api, "uno", {"draining": False})

    # --- nessun ripiego sul server
    patch_node(api, "local", {"role_transcode": False})
    patch_node(api, "uno", {"enabled": False})
    patch_node(api, "due", {"enabled": False})
    api.req("PUT", "/api/v1/admin/settings/nodes", {"fallback_local": False})
    st, r, _ = api.post("/api/v1/playback/info", {"item_id": uno, "profile": H264_PROFILE, "force_transcode": True}, token=login(api, 303))
    check("senza esecutori e senza ripiego: 503", st == 503 and r.get("error") == "TranscodeUnavailable", (st, r))
    api.req("PUT", "/api/v1/admin/settings/nodes", {"fallback_local": True})
    r = play(api, login(api, 304), uno)
    check("con il ripiego transcodifica il server", sessions(api)[r["session_id"]].get("executor") is None, "")
    stop_all(api, [r])
    patch_node(api, "local", {"role_transcode": True})
    patch_node(api, "uno", {"enabled": True})
    patch_node(api, "due", {"enabled": True})
    wait(lambda: node_by_name(api, "uno")["health"] == "online" and node_by_name(api, "due")["health"] == "online", 30, what="nodi di nuovo online")

    # --- CDN: direct play dalla copia del nodo due, failover sul server
    print("== CDN", flush=True)
    patch_node(api, "local", {"cdn_priority": 2})
    d = play(api, login(api, 400), uno, force=False, profile=DIRECT_PROFILE)
    check("direct play", d["method"] == "direct_play", d["method"])
    url = d["stream_url"]
    local = open(os.path.join(work, "media", "film", "Uno (2020)", "Uno (2020).mkv"), "rb").read()
    st, body, hd = api.get(url, raw=True, headers={"Range": "bytes=1000-1999999"})
    check("Range dal nodo identico al file", st == 206 and body == local[1000:2000000] and hd.get("content-range") == f"bytes 1000-1999999/{len(local)}", (st, len(body), hd.get("content-range")))
    check("sessione letta dal nodo due", sessions(api)[d["session_id"]].get("source") == "due", sessions(api)[d["session_id"]].get("source"))
    st, body, _ = api.get(url, raw=True, headers={"Range": f"bytes={len(local) - 100}-"})
    check("coda del file dal nodo", st == 206 and body == local[-100:], (st, len(body)))
    sh("docker", "stop", "-t", "1", N2)
    st, body, _ = api.get(url, raw=True, headers={"Range": "bytes=0-65535"})
    check("nodo spento: stessi byte dal server", st == 206 and body == local[:65536], (st, len(body)))
    check("sessione ora letta dal server", sessions(api)[d["session_id"]].get("source") is None, sessions(api)[d["session_id"]].get("source"))
    sh("docker", "start", N2)
    wait(lambda: node_by_name(api, "due")["health"] == "online", 60, what="due online")
    stop_all(api, [d])

    # --- download dalla copia e consegna diretta
    tok = login(api, 401)
    st, r, _ = api.post(f"/api/v1/items/{uno}/download", {}, token=tok)
    st, body, hd = api.get(r["url"], raw=True, headers={"Range": "bytes=0-1023"})
    check("download dalla copia del nodo", st == 206 and body == local[:1024] and "attachment" in hd.get("content-disposition", ""), (st, hd.get("content-disposition")))
    api.req("PUT", "/api/v1/admin/settings/nodes", {"direct_delivery": True})
    patch_node(api, "due", {"public_url": f"http://127.0.0.1:{N2_PORT}"})
    d = play(api, login(api, 402), uno, force=False, profile=DIRECT_PROFILE)
    st, _, hd = api.req("GET", d["stream_url"], raw=True, follow=False, headers={"Range": "bytes=0-1023"})
    loc = hd.get("location", "")
    check("consegna diretta: redirect firmato verso il nodo", st == 302 and loc.startswith(f"http://127.0.0.1:{N2_PORT}/node/v1/d/"), (st, loc))
    with urllib.request.urlopen(urllib.request.Request(loc, headers={"Range": "bytes=0-1023"}), timeout=30) as resp:
        body = resp.read()
        check("il nodo serve il link firmato", resp.status == 206 and body == local[:1024], resp.status)
    tampered = loc.replace("/node/v1/d/", "/node/v1/d/9") if "/d/9" not in loc else loc.replace("/d/9", "/d/8")
    try:
        urllib.request.urlopen(tampered, timeout=30)
        check("link manomesso rifiutato", False, tampered)
    except urllib.error.HTTPError as e:
        check("link manomesso rifiutato", e.code in (401, 403), e.code)
    api.req("PUT", "/api/v1/admin/settings/nodes", {"direct_delivery": False})
    stop_all(api, [d])

    # --- copia diversa sul nodo: esclusa
    victim = os.path.join(work, "copia", "film", "Uno (2020)", "Uno (2020).mkv")
    sh("docker", "run", "--rm", "--user", "0", "--entrypoint", "sh", "-v", f"{work}/copia:/c", a.image, "-c", "truncate -s 1000 '/c/film/Uno (2020)/Uno (2020).mkv'")
    sh("docker", "restart", "-t", "1", SRV)  # cache delle verifiche azzerata
    wait(lambda: api.get("/api/v1/health")[0] == 200, 60, what="server di nuovo pronto")
    wait(lambda: node_by_name(api, "due")["health"] == "online", 60, what="due online dopo il riavvio del server")
    d = play(api, login(api, 403), uno, force=False, profile=DIRECT_PROFILE)
    st, body, _ = api.get(d["stream_url"], raw=True, headers={"Range": "bytes=0-1023"})
    check("copia troncata sul nodo: si legge dal server", st == 206 and body == local[:1024] and sessions(api)[d["session_id"]].get("source") is None, (st, sessions(api)[d["session_id"]].get("source")))
    stop_all(api, [d])
    shutil.copyfile(os.path.join(work, "media", "film", "Uno (2020)", "Uno (2020).mkv"), victim) if os.access(victim, os.W_OK) else None

    # --- azioni dalla dashboard
    print("== azioni", flush=True)
    nid = node_by_name(api, "uno")["id"]
    st, r, _ = api.get(f"/api/v1/admin/nodes/{nid}/logs?limit=50")
    check("log del nodo dal server", st == 200 and len(r["lines"]) > 0, st)
    st, r, _ = api.post(f"/api/v1/admin/nodes/{nid}/hw-test")
    check("prova GPU del nodo", st == 200 and r["status"]["caps"]["tested_at"] > 0, st)
    patch_node(api, "uno", {"config": {"hw_backend": "none", "max_sw_transcodes": 1, "sw_preset": "superfast"}})
    check("configurazione nuova arrivata al nodo", bool(wait(lambda: node_by_name(api, "uno")["status"]["config"]["sw_preset"] == "superfast", 20, what="config nuova")), "")
    st, r, _ = api.post(f"/api/v1/admin/nodes/{nid}/restart")
    check("riavvio del nodo chiesto", st == 200, r)
    time.sleep(3)
    sh("docker", "start", N1, check=False)  # senza restart policy nei test
    check("il nodo riavviato torna online", bool(wait(lambda: node_by_name(api, "uno")["health"] == "online", 60, what="uno online")), "")

    # --- server spento: il nodo chiude le transcodifiche orfane
    if not a.skip_orphan:
        print("== server spento", flush=True)
        patch_node(api, "uno", {"tc_priority": 1, "config": {"hw_backend": "auto"}})
        patch_node(api, "local", {"tc_priority": 5})
        o = play(api, login(api, 500), uno)
        segment(api, o, 0)
        check("sessione sul nodo prima dello spegnimento", sessions(api)[o["session_id"]].get("executor") == "uno", "")
        sh("docker", "kill", SRV)
        time.sleep(75)
        ps = sh("docker", "exec", N1, "sh", "-c", "ls /proc | grep -E '^[0-9]+$' | while read p; do cat /proc/$p/comm 2>/dev/null; done", check=False)
        check("senza heartbeat il nodo chiude FFmpeg", "ffmpeg" not in ps and "nessun heartbeat" in logs_of(N1), ps)
        sh("docker", "start", SRV)

    print(f"\n{len(passed)} riuscite, {len(failed)} fallite", flush=True)
    for f_ in failed:
        print(f"  KO {f_}")
    if not a.keep:
        cleanup()
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
