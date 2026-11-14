# TMDB finto per i test: risponde alle chiamate che Platea fa, con dati inventati.
import json, struct, zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

def png(w, h, rgb):
    raw = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))
    def chunk(t, d): return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")

MOVIES = {
    "prova diretta": [{"id": 101, "title": "Prova Diretta", "original_title": "Prova Diretta", "release_date": "2020-05-01", "popularity": 30, "overview": "Un film di prova.", "poster_path": "/p101.png"}],
    "prova remux": [{"id": 102, "title": "Prova Remux", "original_title": "Prova Remux", "release_date": "2021-02-01", "popularity": 20, "poster_path": "/p102.png"}],
    # Due candidati simili: va in "da verificare".
    "prova hevc": [{"id": 103, "title": "Prova HEVC Uno", "release_date": "2019-01-01", "popularity": 5}, {"id": 104, "title": "Prova HEVC Due", "release_date": "2023-01-01", "popularity": 5}],
}

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def send(self, obj, ctype="application/json", code=200):
        body = obj if isinstance(obj, bytes) else json.dumps(obj).encode()
        self.send_response(code); self.send_header("Content-Type", ctype); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
    def do_GET(self):
        u = urlparse(self.path); q = parse_qs(u.query); p = u.path
        if p.startswith("/img/"):
            return self.send(png(60, 90, (200, 40, 40)), "image/png")
        if p == "/3/configuration":
            return self.send({"images": {}})
        if p == "/3/search/movie":
            return self.send({"results": MOVIES.get(q.get("query", [""])[0].lower(), [])})
        if p == "/3/search/tv":
            return self.send({"results": [{"id": 201, "name": "Serie di Prova", "original_name": "Serie di Prova", "first_air_date": "2024-01-01", "popularity": 10, "poster_path": "/s201.png"}]})
        if p.startswith("/3/movie/"):
            mid = int(p.split("/")[3])
            return self.send({"id": mid, "title": f"Film {mid} (TMDB)", "original_title": "Orig", "release_date": "2020-05-01", "runtime": 1, "overview": "Trama da TMDB.", "tagline": "Una frase.", "vote_average": 7.5,
                "poster_path": f"/p{mid}.png", "backdrop_path": f"/b{mid}.png", "genres": [{"name": "Dramma"}], "production_companies": [{"name": "Studio Prova"}],
                "credits": {"cast": [{"id": 9001, "name": "Attrice Prova", "character": "Protagonista", "profile_path": "/a9001.png"}], "crew": [{"id": 9002, "name": "Regista Prova", "job": "Director"}]},
                "images": {"logos": []}, "external_ids": {"imdb_id": "tt0000101"}, "release_dates": {"results": [{"iso_3166_1": "IT", "release_dates": [{"certification": "T"}]}]}})
        if p == "/3/tv/201":
            return self.send({"id": 201, "name": "Serie di Prova (TMDB)", "first_air_date": "2024-01-01", "overview": "Serie da TMDB.", "poster_path": "/s201.png", "backdrop_path": "/sb201.png", "genres": [{"name": "Commedia"}], "networks": [], "aggregate_credits": {"cast": [], "crew": []}, "images": {"logos": []}, "external_ids": {}, "content_ratings": {"results": []}, "episode_run_time": [20], "status": "Returning Series"})
        if p == "/3/tv/201/season/1":
            return self.send({"name": "Stagione 1", "overview": "Prima stagione.", "air_date": "2024-01-01", "poster_path": "/ss1.png",
                "episodes": [{"episode_number": 1, "name": "Pilota", "overview": "Il primo.", "air_date": "2024-01-01", "still_path": "/e1.png"}, {"episode_number": 2, "name": "Il seguito", "air_date": "2024-01-08", "still_path": "/e2.png"}]})
        return self.send({"status_message": "not found"}, code=404)

ThreadingHTTPServer(("0.0.0.0", 8765), H).serve_forever()
