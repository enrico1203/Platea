#!/usr/bin/env python3
"""Test di carico: centinaia di player in direct play con richieste Range e salti casuali,
più alcuni "navigatori" che usano l'API intanto. Misura le latenze: il server non deve bloccarsi.

Uso (dentro un container, rete host):
  docker run --rm --network host -v $PWD/scripts/loadtest:/t python:3.13-slim \
    sh -c "pip -q install aiohttp && python /t/loadtest.py --players 100 --seconds 60 --item <id> --library <id> --prefix a"
Un solo processo Python satura una CPU sopra i ~500 Range/s: per carichi più alti lanciare più processi
con prefissi diversi (un nuovo accesso con lo stesso ID dispositivo revoca il token precedente).
"""
import argparse, asyncio, random, statistics, time
import aiohttp

def pct(v, p):
    if not v:
        return 0.0
    v = sorted(v)
    return v[min(len(v) - 1, int(len(v) * p))] * 1000

async def login(s, base, user, pw, dev):
    async with s.post(f"{base}/api/v1/auth/login", json={"username": user, "password": pw, "device_id": dev, "device_name": dev}) as r:
        r.raise_for_status()
        return (await r.json())["token"]

async def player(s, base, item, token, until, stats):
    h = {"Authorization": f"Bearer {token}"}
    async with s.post(f"{base}/api/v1/playback/info", json={"item_id": item, "profile": {"name": "load"}}, headers=h) as r:
        r.raise_for_status()
        info = await r.json()
    url = base + info["stream_url"]
    async with s.head(url) as r:
        size = int(r.headers["Content-Length"])
    chunk = 512 * 1024
    pos = 0
    while time.monotonic() < until:
        if random.random() < 0.1:
            pos = random.randrange(0, max(1, size - chunk))  # salto
            stats["seeks"] += 1
        end = min(size - 1, pos + chunk - 1)
        t0 = time.monotonic()
        try:
            async with s.get(url, headers={"Range": f"bytes={pos}-{end}"}) as r:
                body = await r.read()
                if r.status != 206:
                    stats["errors"] += 1
                    stats["codes"][f"range {r.status}"] = stats["codes"].get(f"range {r.status}", 0) + 1
                stats["bytes"] += len(body)
        except Exception as e:
            stats["errors"] += 1
            stats["codes"][type(e).__name__] = stats["codes"].get(type(e).__name__, 0) + 1
            continue
        stats["range"].append(time.monotonic() - t0)
        pos = end + 1 if end + 1 < size else 0
        await asyncio.sleep(random.uniform(0.2, 1.0))

async def browser(s, base, token, lib, until, stats):
    h = {"Authorization": f"Bearer {token}"}
    paths = [f"/api/v1/items?library_id={lib}&limit=60", "/api/v1/home", "/api/v1/search?q=amore", f"/api/v1/items?library_id={lib}&sort=year&order=desc&limit=60", "/api/v1/system/info"]
    while time.monotonic() < until:
        p = random.choice(paths)
        t0 = time.monotonic()
        try:
            async with s.get(base + p, headers=h) as r:
                await r.read()
                if r.status != 200:
                    stats["errors"] += 1
                    stats["codes"][f"{p} {r.status}"] = stats["codes"].get(f"{p} {r.status}", 0) + 1
        except Exception as e:
            stats["errors"] += 1
            stats["codes"][type(e).__name__] = stats["codes"].get(type(e).__name__, 0) + 1
            continue
        stats["api"].append(time.monotonic() - t0)
        await asyncio.sleep(random.uniform(0.05, 0.3))

async def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", default="http://127.0.0.1:8096")
    ap.add_argument("--user", default="admin")
    ap.add_argument("--password", default="password123")
    ap.add_argument("--item", required=True)
    ap.add_argument("--library", required=True)
    ap.add_argument("--players", type=int, default=300)
    ap.add_argument("--browsers", type=int, default=20)
    ap.add_argument("--seconds", type=int, default=60)
    ap.add_argument("--prefix", default="load", help="prefisso degli ID dispositivo: diverso per ogni processo in parallelo")
    a = ap.parse_args()
    conn = aiohttp.TCPConnector(limit=0)
    async with aiohttp.ClientSession(connector=conn, timeout=aiohttp.ClientTimeout(total=30)) as s:
        t0 = time.monotonic()
        sem = asyncio.Semaphore(16)
        async def lg(i):
            async with sem:
                return await login(s, a.base, a.user, a.password, f"{a.prefix}-{i}")
        tokens = await asyncio.gather(*(lg(i) for i in range(a.players + 1)))
        print(f"{len(tokens)} login in {time.monotonic() - t0:.1f} s")
        stats = {"range": [], "api": [], "bytes": 0, "errors": 0, "seeks": 0, "codes": {}}
        until = time.monotonic() + a.seconds
        start = time.monotonic()
        tasks = [player(s, a.base, a.item, tokens[i], until, stats) for i in range(a.players)]
        tasks += [browser(s, a.base, tokens[-1], a.library, until, stats) for _ in range(a.browsers)]
        res = await asyncio.gather(*tasks, return_exceptions=True)
        failed = [r for r in res if isinstance(r, Exception)]
        el = time.monotonic() - start
        print(f"player: {a.players}, navigatori: {a.browsers}, durata {el:.0f} s, task falliti {len(failed)} {failed[:2]}")
        print(f"richieste Range: {len(stats['range'])} ({len(stats['range'])/el:.0f}/s), salti {stats['seeks']}, {stats['bytes']/el/1e6*8:.0f} Mb/s")
        print(f"  latenza Range  p50 {pct(stats['range'],.5):.1f} ms  p95 {pct(stats['range'],.95):.1f} ms  p99 {pct(stats['range'],.99):.1f} ms  max {pct(stats['range'],1):.1f} ms")
        print(f"richieste API: {len(stats['api'])} ({len(stats['api'])/el:.0f}/s)")
        print(f"  latenza API    p50 {pct(stats['api'],.5):.1f} ms  p95 {pct(stats['api'],.95):.1f} ms  p99 {pct(stats['api'],.99):.1f} ms  max {pct(stats['api'],1):.1f} ms")
        print(f"errori: {stats['errors']} {stats['codes']}")

asyncio.run(main())
