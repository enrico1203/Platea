/** Lingue disponibili. Per aggiungerne una: un nuovo file di dizionario e una riga qui. */
import type { Messages } from "./index";

export const LOCALES = {
  it: { name: "Italiano", tag: "it-IT", load: () => import("./it").then((m) => m.default as Messages) },
  en: { name: "English", tag: "en-GB", load: () => import("./en").then((m) => m.default as Messages) },
} as const;

export type LocaleCode = keyof typeof LOCALES;
