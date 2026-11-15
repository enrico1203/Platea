// Adatta l'uscita di ts-json-schema-generator ai "components.schemas" di OpenAPI:
// nomi validi (Page<Item> → PageItem), riferimenti #/components/schemas/…, chiavi in ordine stabile.
import { readFileSync, writeFileSync } from "node:fs";
const [, , input, output] = process.argv;
const raw = JSON.parse(readFileSync(input, "utf8"));
const clean = (name) => name.replace(/[^A-Za-z0-9._-]/g, "");
const rename = (ref) => {
  const name = decodeURIComponent(ref.replace("#/definitions/", ""));
  return `#/components/schemas/${clean(name)}`;
};
const walk = (v) => {
  if (Array.isArray(v)) return v.map(walk);
  if (v && typeof v === "object") {
    const out = {};
    for (const k of Object.keys(v).sort()) out[k] = k === "$ref" ? rename(v[k]) : walk(v[k]);
    return out;
  }
  return v;
};
const schemas = {};
for (const name of Object.keys(raw.definitions).sort()) schemas[clean(name)] = walk(raw.definitions[name]);
writeFileSync(output, JSON.stringify(schemas, null, 1) + "\n");
console.log(`${Object.keys(schemas).length} schemi`);
