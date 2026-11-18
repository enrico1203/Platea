"""Controlla le risposte di Platea contro lo schema OpenAPI di Jellyfin, con le regole dell'SDK Kotlin usato da
Findroid e da altre app: campi non nullable obbligatori, niente null dove non ammessi, enum chiusi, UUID, date.

  curl -sLo openapi.json https://repo.jellyfin.org/files/openapi/stable/jellyfin-openapi-12.4.json
  python3 scripts/jellyfin-check.py openapi.json http://127.0.0.1:8096 <token> GET /Users/Me \
      POST /Users/AuthenticateByName '{"Username":"u","Pw":"p"}' GET "/Items/<id>?userId=<uid>" ...

Dopo un AuthenticateByName usa il token nuovo (lo scrive anche in ./tok)."""
import json, re, sys, urllib.request

spec = json.load(open(sys.argv[1]))
S = spec["components"]["schemas"]
BASE, TOKEN = sys.argv[2], sys.argv[3]
UUID = re.compile(r"^[0-9a-fA-F]{8}-?[0-9a-fA-F]{4}-?[0-9a-fA-F]{4}-?[0-9a-fA-F]{4}-?[0-9a-fA-F]{12}$")
DATE = re.compile(r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(\.\d+)?(Z|[+-]\d\d:\d\d)?$")


def deref(sch):
    while True:
        if "$ref" in sch:
            sch = S[sch["$ref"].split("/")[-1]]
        elif "allOf" in sch and len(sch["allOf"]) == 1:
            sch = sch["allOf"][0]
        elif "oneOf" in sch and len(sch["oneOf"]) == 1:
            sch = sch["oneOf"][0]
        else:
            return sch


def check(v, sch, where, errs, nullable=False):
    nullable = nullable or sch.get("nullable", False)
    sch2 = deref(sch)
    nullable = nullable or sch2.get("nullable", False)
    if v is None:
        if not nullable:
            errs.add(f"{where}: null non ammesso")
        return
    if "oneOf" in sch2 or "anyOf" in sch2:
        return
    t = sch2.get("type")
    if "enum" in sch2:
        if v not in sch2["enum"]:
            errs.add(f"{where}: valore enum sconosciuto {v!r}")
        return
    if t == "object" or "properties" in sch2:
        if not isinstance(v, dict):
            errs.add(f"{where}: atteso oggetto, trovato {type(v).__name__}")
            return
        props = sch2.get("properties", {})
        for k, ps in props.items():
            pn = ps.get("nullable", False) or deref(ps).get("nullable", False)
            if k not in v and not pn:
                errs.add(f"{where}.{k}: obbligatorio, manca")
            elif k in v:
                check(v[k], ps, f"{where}.{k}", errs, pn)
        ap = sch2.get("additionalProperties")
        if isinstance(ap, dict) and not props:
            for k, x in v.items():
                check(x, ap, f"{where}[*]", errs)
    elif t == "array":
        if not isinstance(v, list):
            errs.add(f"{where}: atteso array")
            return
        for x in v[:200]:
            check(x, sch2.get("items", {}), f"{where}[]", errs)
    elif t == "string":
        if not isinstance(v, str):
            errs.add(f"{where}: attesa stringa, trovato {v!r}"[:200])
        elif sch2.get("format") == "uuid" and not UUID.match(v):
            errs.add(f"{where}: UUID non valido {v!r}")
        elif sch2.get("format") == "date-time" and not DATE.match(v):
            errs.add(f"{where}: data non valida {v!r}")
    elif t == "integer":
        if not isinstance(v, int) or isinstance(v, bool):
            errs.add(f"{where}: atteso intero, trovato {v!r}"[:200])
    elif t == "number":
        if not isinstance(v, (int, float)) or isinstance(v, bool):
            errs.add(f"{where}: atteso numero, trovato {v!r}"[:200])
    elif t == "boolean":
        if not isinstance(v, bool):
            errs.add(f"{where}: atteso booleano, trovato {v!r}")


def find_op(method, path):
    p = path.split("?")[0]
    best = None
    for tpl, ops in spec["paths"].items():
        rx = "^" + re.sub(r"\{[^}]+\}", "[^/]+", tpl) + "$"
        if re.match(rx, p, re.I) and method.lower() in ops:
            score = tpl.count("{")
            if best is None or score < best[0]:
                best = (score, tpl, ops[method.lower()])
    return best


args = sys.argv[4:]
i = 0
total = 0
while i < len(args):
    method, path = args[i], args[i + 1]
    i += 2
    body = None
    if i < len(args) and args[i].startswith(("{", "[")):
        body = args[i].encode()
        i += 1
    op = find_op(method, path)
    req = urllib.request.Request(BASE + path, data=body, method=method)
    req.add_header("Authorization", f'MediaBrowser Client="Findroid", Device="Test", DeviceId="t1", Version="0.15", Token="{TOKEN}"')
    if body:
        req.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            status, raw, ctype = r.status, r.read(), r.headers.get("Content-Type", "")
    except urllib.error.HTTPError as e:
        status, raw, ctype = e.code, e.read(), ""
    if op is None:
        print(f"!! {method} {path}: percorso non nello schema ({status})")
        continue
    _, tpl, o = op
    resp = o.get("responses", {})
    ok = [c for c in resp if c.startswith("2")]
    if str(status) not in ok:
        print(f"!! {method} {path} [{tpl}]: stato {status}, attesi {ok} :: {raw[:150]!r}")
        continue
    content = resp[str(status)].get("content", {})
    sch = (content.get("application/json") or {}).get("schema")
    if not sch:
        print(f"ok {method} {path} [{status}, nessun corpo JSON atteso]")
        continue
    try:
        v = json.loads(raw)
    except Exception:
        print(f"!! {method} {path}: risposta non JSON ({ctype}) {raw[:100]!r}")
        continue
    if isinstance(v, dict) and v.get("AccessToken"):
        TOKEN = v["AccessToken"]
        open("tok", "w").write(TOKEN)
    errs = set()
    check(v, sch, "$", errs)
    total += len(errs)
    print(("!! " if errs else "ok ") + f"{method} {path} [{tpl}]" + (f": {len(errs)} problemi" if errs else ""))
    for e in sorted(errs)[:60]:
        print("     ", e)
print("problemi totali:", total)
