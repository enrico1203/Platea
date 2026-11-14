# Platea — Brand identity

Open source media server

Platea è un open source media server. Palette nera, bianca e blu, tema scuro predefinito: nero per i fondi, bianco per il testo, un solo blu per lo schermo del marchio e per l'azione principale.

## Il nome e il marchio

La platea è la parte della sala dove sta il pubblico, davanti allo schermo. Il marchio è una **P** il cui occhiello è uno schermo blu 16:9: la lettera del nome e lo schermo sono la stessa forma.

- Scrivi il nome **Platea** con l'iniziale maiuscola nel testo. Il wordmark del logo è tutto minuscolo (`platea`): non riscriverlo a mano, usa i file del gruppo Logo.
- Il lockup orizzontale (`platea-lockup.svg`) è la firma predefinita: marchio alto quanto le ascendenti, la `p` del wordmark scende sotto la linea di base e fa rima con il marchio.
- Il marchio da solo (`platea-mark.svg`) va dove lo spazio è quadrato: favicon, barra laterale chiusa, avatar del server nei client. Sotto i 24px usa l'icona dell'app.
- L'icona dell'app (`platea-app-icon.svg`) è il marchio su un quadrato `bg` con angoli `radius-lg` alla scala 64. Per iOS e Android esporta l'SVG senza il quadrato e lascia gli angoli al sistema.
- Spazio di rispetto intorno al marchio: almeno l'altezza dell'occhiello blu su ogni lato.
- Lo schermo del marchio è sempre `blue`. Le versioni monocromatiche (`platea-mark-white.svg`, `platea-mark-black.svg`) lasciano l'occhiello vuoto: si usano solo dove il blu non è disponibile, come la stampa a un colore o il ricamo.
- Non ruotare, non mettere ombre o contorni, non riempire lo schermo con immagini, non cambiare il blu con un altro colore.

## Tono di voce

Platea parla come un tecnico bravo che non ha fretta: dice cosa succede e perché, con i numeri veri. Mai entusiasmo da marketing.

- Italiano, seconda persona singolare, frasi brevi all'indicativo. "Il server ha trovato 3 GPU." Non "Fantastico! Abbiamo rilevato…".
- Ogni stato ha un motivo. Non "Errore di riproduzione", ma "Transcodifica: il browser non decodifica HEVC 10 bit".
- I termini tecnici restano quelli dell'API: Direct play, Remux, HLS, codec, bitrate. I codici motivo restano in inglese, in `mono`: `VideoCodecNotSupported`.
- Pulsanti con un verbo all'infinito: Riproduci, Scansiona, Identifica, Revoca. Niente maiuscolo integrale, tranne le etichette `eyebrow`.
- Numeri all'italiana: virgola decimale e unità con spazio, `58,4 Mb/s`, `2 h 46 min`, `1,2 TB`. Le cifre nelle tabelle sono tabellari.
- Niente emoji nell'interfaccia e nella documentazione.
- Promesse misurabili. "Zero file aperti in una scansione senza modifiche" sì; "velocissimo" no.

Nessuno slogan, né nell'interfaccia né nei materiali. Dove serve una riga sotto il nome si scrive solo **Open source media server**, in inglese, senza punto finale.

## Colore

Il tema **scuro** è il predefinito ovunque: web UI, dashboard, sito, documentazione. Il tema chiaro esiste per chi lo sceglie e per la stampa, con gli stessi nomi di token.

- Il fondo è `bg`, quasi nero, mai `#000`. La profondità si costruisce salendo di superficie: `bg` → `surface-1` (pannelli, barra laterale) → `surface-2` (campi, hover) → `surface-3` (premuto, menu aperti).
- Il testo è `ink` per ciò che si legge, `ink-muted` per i metadati, `ink-subtle` per segnaposto ed etichette `eyebrow`. Tutti e tre superano 4.5:1 su ogni superficie in entrambi i temi.
- **Un solo blu per vista.** `blue` è lo schermo: il pulsante primario, il Play, l'avanzamento, l'occhiello del marchio. Se in una schermata ci sono due cose blu piene, una delle due è sbagliata. `blue-text` per link e voce attiva, `blue-soft` per lo sfondo della selezione.
- Sul blu pieno il testo è sempre `on-blue`.
- I tre metodi di riproduzione hanno un colore ciascuno: `method-direct` (verde, il risultato ideale), `method-remux` (blu, costo minimo), `method-transcode` (ambra, il server lavora). Il colore accompagna sempre l'etichetta scritta, mai da solo.
- `success`, `warning`, `danger` solo per stati e messaggi, con i fondi `-soft` nei badge. Mai come colori decorativi.
- Vietati: sfumature blu-viola, bagliori colorati diffusi sul fondo, testo grigio su blu.

## Tipografia

Una sola famiglia, **Geist**, con **Geist Mono** per tutto ciò che è un dato tecnico. Entrambe sono incluse come file variabili (licenza SIL OFL).

- Titoli in `display`, `h1`, `h2`, `h3`: peso 600 e spaziatura negativa, più stretta quanto più grande è il corpo. Il titolo nella pagina di dettaglio è `display`.
- Testo corrente in `body`; trame in `body-lg`; metadati in `small`; pulsanti ed etichette in `label`.
- `eyebrow` sopra i gruppi della dashboard, in maiuscolo con `text-transform: uppercase` e `ink-subtle`.
- `mono` per codec, risoluzioni, bitrate, percorsi, codici motivo e log; `mono-sm` dentro i badge; `numeral` per i grandi numeri della Panoramica.
- Mai più di tre livelli di testo in una card: titolo, metadati, tecnico.

## Spazi, angoli, superfici

- Griglia da 4px. Margine laterale `space-4` su telefono e `space-6` su desktop; padding delle card `space-4`, dei pannelli e dialoghi `space-5`; distanza tra le righe di locandine `space-7`.
- Angoli: `radius-xs` per badge e checkbox, `radius-sm` per locandine (lo schermo resta quasi squadrato), `radius-md` per pulsanti e campi, `radius-lg` per card, pannelli e dialoghi, `radius-full` per switch, barre e il Play.
- Bordi prima delle ombre: le card hanno un filo `line-strong`, i separatori sono `line`, i controlli `line-control`. L'ombra `shadow-pop` solo per ciò che galleggia (menu, dialoghi).
- `shadow-glow` è la luce dello schermo: solo sul Play principale e sulla locandina a fuoco con il telecomando.
- Focus: anello `focus` pieno di 2px con 2px di distacco, su ogni elemento interattivo. Mai rimosso.

## Immagini e locandine

- Le locandine e gli sfondi vengono da TMDB e sono il colore dell'interfaccia: l'interfaccia resta neutra perché le immagini parlano.
- Prima che un'immagine arrivi, il suo posto ha il colore segnaposto salvato con l'immagine, o `poster-placeholder`. Niente skeleton animati e niente spinner sulle griglie.
- Lo sfondo nella pagina di dettaglio scende verso `bg` con un velo `scrim`; il testo sopra resta `ink`.
- Le foto degli attori sono cerchi `radius-full`, scaricate solo quando la pagina le mostra.
- Attribuzione TMDB nella pagina "Informazioni" di ogni client, con la dicitura che il prodotto non è approvato da TMDB.

## Movimento

- Transizioni brevi, 120–200 ms, con uscita morbida. Cambiano colore, opacità e posizione di pochi pixel, mai la scala di un'intera pagina.
- Il player nasconde i controlli dopo 3 secondi di inattività con una dissolvenza.
- Con `prefers-reduced-motion` le transizioni diventano istantanee.

## Iconografia

- Icone [Lucide](https://lucide.dev), contorno 1.75px, 20px nell'interfaccia e 18px nei pulsanti, colore `currentColor`. Il Play e i controlli del player sono pieni.
- Un'icona accompagna un testo; da sola solo in pulsanti quadrati con `aria-label`.
- Nessuna emoji, nessuna illustrazione decorativa. Gli stati vuoti usano il marchio monocromatico in `ink-subtle` e una frase che dice cosa fare.

## Componenti

I componenti sono classi CSS (`components/bundle.css`) con prefisso `pl-`, dentro un contenitore `pl-root`. Ogni valore viene dai token: Button, TextField, Switch, MethodBadge, PosterCard, SessionRow. La web UI li reimplementa in React con gli stessi nomi; le app native (Kotlin, Swift) usano gli stessi token.

Nelle app Apple con Liquid Glass le barre di sistema prendono il materiale di sistema: il blu del marchio resta sui controlli del player e sull'azione principale, i fondi restano quelli di sistema.

## Token

### Colori

| Token | Scuro (predefinito) | Chiaro | Uso |
|---|---|---|---|
| `bg` | `#050608` | `#f6f7f9` | Fondo della pagina e della home. Nel tema scuro è quasi nero, mai #000 puro. |
| `surface-1` | `#0c0e11` | `#ffffff` | Barre laterali, pannelli, schede della dashboard su `bg`. |
| `surface-2` | `#14171b` | `#eef0f3` | Campi di input, righe in hover, card dentro un pannello. |
| `surface-3` | `#1c2026` | `#e4e7ec` | Elementi premuti, menu aperti, traccia dello switch spento. |
| `line` | `#22262d` | `#dde1e6` | Linee sottili decorative: separatori tra righe e sezioni. Non per i bordi dei controlli. |
| `line-strong` | `#2f343c` | `#c9ced5` | Bordo di card e pannelli su `bg`. |
| `line-control` | `#68707c` | `#7f8793` | Bordo di input, checkbox e switch: almeno 3:1 su `surface-1`…`surface-3`. |
| `ink` | `#f3f5f7` | `#0a0c0f` | Testo principale e titoli su ogni superficie (≥ 15:1). |
| `ink-muted` | `#a2a9b4` | `#4a525e` | Testo secondario: trame, metadati, descrizioni su ogni superficie (≥ 6.3:1). |
| `ink-subtle` | `#808893` | `#5f6773` | Testo terziario e segnaposto su ogni superficie (≥ 4.5:1). Mai per testo da leggere a lungo. |
| `blue` | `#2f66f5` | `#1f5cf0` | Platea Blue, il blu dello schermo. Colore del marchio e riempimento dei pulsanti primari con `on-blue`. Usarlo poco: un elemento blu per vista. |
| `blue-hover` | `#2556e0` | `#1a4fd2` | Hover e pressione dei pulsanti primari. |
| `on-blue` | `#ffffff` | `#ffffff` | Testo e icone su `blue` e `blue-hover` (≥ 4.8:1). |
| `blue-text` | `#6e9bff` | `#1a53d6` | Link, voce di navigazione attiva, valori interattivi su ogni superficie (≥ 5.2:1). |
| `blue-soft` | `#0f1d3d` | `#e5edff` | Fondo della selezione e dell'elemento attivo; ci si scrive sopra in `ink` o `blue-text`. |
| `focus` | `#6e9bff` | `#1a53d6` | Anello di focus: 2px pieno con 2px di distacco, ≥ 5:1 su ogni superficie. |
| `success` | `#3ccf8e` | `#0a7347` | Esito positivo, direct play, scansione completata. Testo su ogni superficie. |
| `warning` | `#f2b33d` | `#8a5300` | Attenzione e transcodifica. Testo su ogni superficie. |
| `danger` | `#ff6b6b` | `#b92424` | Errore, azioni distruttive, mount assente. Testo su ogni superficie. |
| `success-soft` | `#3ccf8e1f` | `#0a73471a` | Fondo dei badge di successo, sotto testo `success`. |
| `warning-soft` | `#f2b33d1f` | `#8a53001a` | Fondo dei badge di attenzione, sotto testo `warning`. |
| `danger-soft` | `#ff6b6b1f` | `#b924241a` | Fondo dei badge di errore, sotto testo `danger`. |
| `method-direct` | `{success}` | `{success}` | Metodo di riproduzione Direct play: il file originale. Sempre con l'etichetta scritta. |
| `method-remux` | `{blue-text}` | `{blue-text}` | Metodo Remux: cambia il contenitore, il video è copiato. Sempre con l'etichetta scritta. |
| `method-transcode` | `{warning}` | `{warning}` | Metodo Transcodifica: il server ricodifica. Sempre con l'etichetta e il codice motivo. |
| `scrim` | `#000000b8` | `#0a0c0f99` | Velo sotto i controlli del player e sotto i dialoghi. |
| `poster-placeholder` | `#1a1d22` | `#d9dde3` | Fondo della locandina finché l'immagine non arriva, se manca il colore segnaposto dell'immagine. |

### Tipografia

Famiglie: `sans`: Geist, ui-sans-serif, system-ui, sans-serif; `mono`: "Geist Mono", ui-monospace, SFMono-Regular, Menlo, monospace.

File: `fonts/Geist-Variable.woff2` (Geist, 100 900), `fonts/GeistMono-Variable.woff2` (Geist Mono, 100 900). Licenza SIL OFL.

| Stile | Famiglia | Corpo / interlinea | Peso | Spaziatura | Esempio | Uso |
|---|---|---|---|---|---|---|
| `display-xl` | sans | 72px / 72px | 600 | -0.045em | Open source media server | Una sola volta per pagina: sito, schermata di benvenuto, fine della procedura guidata. |
| `display` | sans | 48px / 52px | 600 | -0.04em | Dune: Parte due | Titolo nella pagina di dettaglio di film e serie. |
| `h1` | sans | 32px / 38px | 600 | -0.03em | Librerie | Titolo di pagina della web UI e della dashboard. |
| `h2` | sans | 24px / 30px | 600 | -0.02em | Continua a guardare | Titolo di sezione e di riga di locandine. |
| `h3` | sans | 18px / 24px | 600 | -0.01em | Transcodifica hardware | Titolo di card, pannello e dialogo. |
| `body-lg` | sans | 17px / 26px | 400 | 0 | Un giovane erede viaggia verso il pianeta più pericoloso dell'universo. | Trama nella pagina di dettaglio. |
| `body` | sans | 15px / 22px | 400 | 0 | La scansione legge solo l'elenco dei file, non il loro contenuto. | Testo corrente dell'interfaccia. |
| `label` | sans | 14px / 20px | 500 | 0 | Riproduci | Pulsanti, voci di menu, etichette dei campi. |
| `small` | sans | 13px / 18px | 400 | 0 | 2024 · 2 h 46 min · 13+ | Metadati sotto i titoli, aiuti dei campi. |
| `eyebrow` | sans | 11px / 14px | 600 | 0.08em | RIPRODUZIONI ATTIVE | Etichetta sopra un gruppo, sempre in maiuscolo con `text-transform: uppercase`. In `ink-subtle`. |
| `mono` | mono | 13px / 18px | 400 | 0 | HEVC Main 10 · 2160p · 58,4 Mb/s | Codec, bitrate, risoluzioni, percorsi, codici motivo, log. |
| `mono-sm` | mono | 12px / 16px | 500 | 0 | VideoCodecNotSupported | Codici motivo nei badge e valori nelle tabelle dense. |
| `numeral` | mono | 28px / 32px | 500 | -0.02em | 412 Mb/s | Numeri grandi della Panoramica: banda, stream attivi, uso della GPU. |

### Spaziature

| Token | Valore | Uso |
|---|---|---|
| `space-1` | 4px | Distanza tra icona e testo nei badge. |
| `space-2` | 8px | Distanza tra icona e testo nei pulsanti; tra locandina e titolo. |
| `space-3` | 12px | Padding verticale delle righe di tabella; gap tra locandine su telefono. |
| `space-4` | 16px | Padding delle card, margine laterale su telefono, gap tra locandine su desktop. |
| `space-5` | 24px | Padding dei pannelli e dei dialoghi. |
| `space-6` | 32px | Distanza tra sezioni dentro una pagina; margine laterale su desktop. |
| `space-7` | 48px | Distanza tra righe di locandine nella home. |
| `space-8` | 64px | Respiro sopra il titolo nella pagina di dettaglio. |

### Raggi

| Token | Valore | Uso |
|---|---|---|
| `radius-xs` | 4px | Badge, tag dei codec, checkbox. |
| `radius-sm` | 6px | Locandine e miniature: lo schermo resta quasi squadrato. |
| `radius-md` | 10px | Pulsanti, campi di input, voci di menu. |
| `radius-lg` | 14px | Card, pannelli, dialoghi; anche l'icona dell'app su 64. |
| `radius-full` | 999px | Switch, barra di avanzamento, pulsante Play circolare. |

### Ombre

Nel tema scuro la profondità viene dalle superfici, non dalle ombre: l'ombra serve solo per ciò che galleggia.

| Token | Scuro | Chiaro | Uso |
|---|---|---|---|
| `shadow-pop` | `0 16px 40px rgba(0,0,0,0.6), 0 0 0 1px #2f343c` | `0 16px 40px rgba(10,12,15,0.14), 0 0 0 1px #dde1e6` | Menu, popover e dialoghi. |
| `shadow-glow` | `0 0 0 1px #2f66f5, 0 8px 32px rgba(47,102,245,0.35)` | `0 0 0 1px #1f5cf0, 0 8px 24px rgba(31,92,240,0.2)` | La luce dello schermo: solo sulla locandina a fuoco con il telecomando e sul pulsante Play principale. |

## Logo

Logo di Platea: marchio, wordmark, lockup e icona dell'app. Tutti SVG in tracciati, nessun font richiesto.

- `platea-lockup.svg` — firma predefinita sul tema scuro: marchio e wordmark in `ink` (#f3f5f7), schermo in `blue` (#2f66f5).
- `platea-lockup-light.svg` — firma sul tema chiaro: marchio e wordmark in `ink` chiaro (#0a0c0f), schermo in `blue`.
- `platea-mark.svg` — marchio su fondo scuro: P in #f3f5f7, schermo in #2f66f5.
- `platea-mark-light.svg` — marchio su fondo chiaro: P in #0a0c0f, schermo in #2f66f5.
- `platea-mark-white.svg` — monocromatico, inchiostro #f3f5f7, occhiello vuoto. Solo dove il blu non è disponibile.
- `platea-mark-black.svg` — monocromatico, inchiostro #0a0c0f, occhiello vuoto.
- `platea-app-icon.svg` — marchio su quadrato #050608 con angoli 14/64: favicon, PWA, icone delle app.
- `platea-wordmark-white.svg`, `platea-wordmark-black.svg` — solo il nome, inchiostro #f3f5f7 o #0a0c0f, per i casi in cui il marchio è già presente.

Gli SVG non ereditano il colore: scegli il file con l'inchiostro giusto per il fondo.

## Componenti in dettaglio

Classi CSS con prefisso `pl-` in `components/bundle.css`, dentro un contenitore `pl-root`.

### Button

Il pulsante esegue un'azione; il primario blu è uno solo per vista.

**Varianti** (classe `pl-btn` più una variante):
- `pl-btn--primary`: `blue` con testo `on-blue`. L'azione principale: Riproduci, Salva, Continua.
- `pl-btn--secondary`: `surface-2` con bordo `line-strong`. Azioni accanto al primario: Scansiona, Identifica.
- `pl-btn--ghost`: senza fondo, testo `ink-muted`. Annulla, azioni nelle barre.
- `pl-btn--danger`: fondo `danger-soft`, testo `danger`. Revoca, Elimina, sempre dietro una conferma.
- `pl-btn--play`: cerchio da 56px in `blue` con `shadow-glow`. Solo nel dettaglio e nel player.

**Misure**: `pl-btn--sm` 32px (tabelle della dashboard), predefinito 40px, `pl-btn--lg` 48px (dettaglio, telefono). `pl-btn--icon` rende il pulsante quadrato: serve sempre `aria-label`.

**Il consumatore fornisce**: l'etichetta con un verbo all'infinito ("Riproduci", non "Riproduzione"), l'icona opzionale prima del testo (SVG 18px, `currentColor`), `disabled` quando l'azione non è possibile.

**Da non fare**: due primari affiancati; testo tutto maiuscolo; un pulsante blu dentro una card blu.

### TextField

Il campo di testo raccoglie un valore con etichetta sempre visibile sopra.

Struttura: `label.pl-field` › `.pl-field__label`, `.pl-field__control` (con `input` e icona opzionale a sinistra), `.pl-field__hint` opzionale.

- Fondo `surface-2`, bordo `line-control`; a fuoco bordo e anello `focus`.
- `is-mono` sul controllo per percorsi, chiavi, URL e numeri tecnici: li scrive in `mono`.
- `is-error` sul campo: bordo e aiuto in `danger`. Il messaggio dice cosa è successo e cosa fare, non "Valore non valido".
- I segreti (chiave TMDB) si scrivono ma non si rileggono: dopo il salvataggio il campo mostra "Impostata" e un pulsante Sostituisci.
- **Mai zoom al tocco su iOS.** Safari ingrandisce la pagina quando si tocca un campo con testo sotto i 16px. Su dispositivi touch e sotto i 900px ogni `input`, `select`, `textarea` ha `font-size` di almeno **16px**, qualunque sia la sua classe (anche `is-mono`): la regola globale in `app.css` lo impone con `!important`. Su desktop resta 14px (13px mono). Lo zoom non si blocca mai con `maximum-scale` o `user-scalable=no`: serve all'accessibilità.

**Il consumatore fornisce**: etichetta, segnaposto di esempio, aiuto quando il formato non è ovvio.

### Switch

Lo switch accende o spegne un'impostazione che si applica subito, senza pulsante Salva.

- Acceso: traccia `blue`, pomello `on-blue`. Spento: traccia `surface-3`, bordo `line-control`, pomello `ink-muted`.
- L'etichetta a destra descrive lo stato acceso ("Transcodifica hardware"), mai una domanda.
- Usa `role="switch"` e `aria-checked`; si attiva con clic, Spazio e Invio.
- Se l'impostazione richiede un riavvio, usa una casella con Salva e mostra l'avviso: lo switch promette un effetto immediato.

**Il consumatore fornisce**: etichetta, stato, gestore del cambio.

### MethodBadge

Il badge mostra uno stato breve; la variante di metodo dice come viene servito un file.

**Metodo di riproduzione** — il cuore del prodotto, visibile in Panoramica, nelle sessioni e nella cronologia:
- `pl-badge--direct` in `method-direct`: "Direct play". Nessun codice motivo.
- `pl-badge--remux` in `method-remux`: "Remux" più il codice motivo in `pl-badge__code`.
- `pl-badge--transcode` in `method-transcode`: "Transcodifica" più il codice motivo.

Il colore non è mai l'unico segnale: l'etichetta è sempre scritta e il codice motivo (`VideoCodecNotSupported`, `ContainerNotSupported`, `AudioCodecNotSupported`, `BitrateTooHigh`, `SubtitleBurnIn`, `HdrNotSupported`) resta in inglese, come nell'API, in `mono-sm`.

**Altri badge**: `pl-badge--danger` per errori di stato (Mount assente), `pl-badge` neutro per stati di coda, `pl-badge--codec` per i tag tecnici (codec, risoluzione, HDR, audio) in mono con bordo `line-strong`.

**Il consumatore fornisce**: il testo, il codice motivo quando c'è.

### PosterCard

La locandina rappresenta un film, una serie o una stagione nelle griglie e nelle righe della home.

- Proporzione 2:3, angoli `radius-sm`, filo interno `line`.
- Prima che l'immagine arrivi il fondo è il colore segnaposto salvato con l'immagine (tabella `images`), altrimenti `poster-placeholder`. Niente skeleton animati.
- Avanzamento: barra da 4px in `blue` su `scrim`, in basso, solo se la visione è iniziata e non finita.
- Visto: cerchio `blue` con spunta `on-blue` in alto a destra.
- `is-focus` (telecomando, tastiera): filo `blue` e `shadow-glow`, la luce dello schermo. È l'unico posto della libreria dove compare il bagliore.
- Titolo in `ink` su una riga con ellissi; metadati in `small`/`ink-subtle`: anno · durata, oppure "Serie · N stagioni".

**Il consumatore fornisce**: URL dell'immagine ridimensionata, colore segnaposto, titolo, metadati, avanzamento 0–1, visto.

**Griglia**: 3 colonne su telefono con gap `space-3`, da 5 a 8 su desktop con gap `space-4`.

### SessionRow

La riga di sessione mostra una riproduzione attiva nella Panoramica della dashboard, con metodo e motivo.

Colonne: miniatura 40×60, poi titolo (`ink`), utente e client (`ink-muted`), riga tecnica in `mono` (`ink-subtle`) e barra di avanzamento; a destra il badge di metodo e il bitrate in `mono`.

- La riga tecnica descrive la catena: `HEVC 10 bit → H.264 · QSV · Arc A380`. La freccia `→` indica cosa il server ha cambiato.
- Le righe sono separate da `line`; dentro un `pl-panel` (`surface-1`, bordo `line-strong`, `radius-lg`).
- Ordina per metodo: prima le transcodifiche, perché sono quelle su cui l'amministratore può agire.
- I numeri usano cifre tabellari e la virgola decimale italiana: `58,4 Mb/s`.

**Il consumatore fornisce**: elemento, utente, client e dispositivo, catena tecnica, metodo, codice motivo, bitrate, posizione.

### CSS dei componenti

```css
/* Platea — stili dei componenti. Ogni valore viene dai token (tokens.css). */
.pl-root{font-family:var(--font-sans);color:var(--ink);background:var(--bg);-webkit-font-smoothing:antialiased}
.pl-root *{box-sizing:border-box}
.pl-mono{font-family:var(--font-mono);font-variant-numeric:tabular-nums}

/* Button */
.pl-btn{display:inline-flex;align-items:center;justify-content:center;gap:var(--space-2);height:40px;padding:0 var(--space-4);border-radius:var(--radius-md);border:1px solid transparent;font:500 14px/20px var(--font-sans);letter-spacing:-0.005em;cursor:pointer;transition:background-color .12s ease,border-color .12s ease,color .12s ease;white-space:nowrap}
.pl-btn svg{width:18px;height:18px;flex:none}
.pl-btn:focus-visible{outline:2px solid var(--focus);outline-offset:2px}
.pl-btn--primary{background:var(--blue);color:var(--on-blue)}
.pl-btn--primary:hover,.pl-btn--primary.is-hover{background:var(--blue-hover)}
.pl-btn--secondary{background:var(--surface-2);color:var(--ink);border-color:var(--line-strong)}
.pl-btn--secondary:hover,.pl-btn--secondary.is-hover{background:var(--surface-3)}
.pl-btn--ghost{background:none;color:var(--ink-muted)}
.pl-btn--ghost:hover,.pl-btn--ghost.is-hover{background:var(--surface-2);color:var(--ink)}
.pl-btn--danger{background:var(--danger-soft);color:var(--danger)}
.pl-btn--sm{height:32px;padding:0 var(--space-3);font-size:13px;border-radius:var(--radius-xs)}
.pl-btn--lg{height:48px;padding:0 var(--space-5);font-size:15px}
.pl-btn--icon{width:40px;padding:0}
.pl-btn--play{width:56px;height:56px;padding:0;border-radius:var(--radius-full);background:var(--blue);color:var(--on-blue);box-shadow:var(--shadow-glow)}
.pl-btn--play svg{width:22px;height:22px;margin-left:2px}
.pl-btn[disabled]{opacity:.4;cursor:not-allowed}

/* TextField */
.pl-field{display:flex;flex-direction:column;gap:6px;min-width:0}
.pl-field__label{font:500 13px/18px var(--font-sans);color:var(--ink)}
.pl-field__control{display:flex;align-items:center;gap:var(--space-2);height:40px;padding:0 var(--space-3);background:var(--surface-2);border:1px solid var(--line-control);border-radius:var(--radius-md);color:var(--ink-subtle)}
.pl-field__control svg{width:16px;height:16px;flex:none}
.pl-field__control input{all:unset;flex:1;min-width:0;font:400 14px/20px var(--font-sans);color:var(--ink)}
.pl-field__control input::placeholder{color:var(--ink-subtle)}
.pl-field__control.is-mono input{font-family:var(--font-mono);font-size:13px}
.pl-field__control:focus-within,.pl-field__control.is-focus{border-color:var(--focus);box-shadow:0 0 0 1px var(--focus)}
.pl-field__hint{font:400 12px/16px var(--font-sans);color:var(--ink-subtle)}
.pl-field.is-error .pl-field__control{border-color:var(--danger)}
.pl-field.is-error .pl-field__hint{color:var(--danger)}

/* Switch */
.pl-switch{display:inline-flex;align-items:center;gap:var(--space-3);font:400 14px/20px var(--font-sans);color:var(--ink);cursor:pointer}
.pl-switch__track{position:relative;width:36px;height:20px;border-radius:var(--radius-full);background:var(--surface-3);border:1px solid var(--line-control);transition:background-color .15s ease}
.pl-switch__track::after{content:"";position:absolute;top:2px;left:2px;width:14px;height:14px;border-radius:var(--radius-full);background:var(--ink-muted);transition:transform .15s ease,background-color .15s ease}
.pl-switch.is-on .pl-switch__track{background:var(--blue);border-color:var(--blue)}
.pl-switch.is-on .pl-switch__track::after{transform:translateX(16px);background:var(--on-blue)}

/* Badge */
.pl-badge{display:inline-flex;align-items:center;gap:6px;height:24px;padding:0 var(--space-2);border-radius:var(--radius-xs);font:500 12px/16px var(--font-sans);white-space:nowrap;background:var(--surface-3);color:var(--ink-muted)}
.pl-badge__dot{width:6px;height:6px;border-radius:var(--radius-full);background:currentColor;flex:none}
.pl-badge__code{font:500 11px/16px var(--font-mono);padding-left:6px;border-left:1px solid currentColor}
.pl-badge--direct{color:var(--method-direct);background:var(--success-soft)}
.pl-badge--remux{color:var(--method-remux);background:var(--blue-soft)}
.pl-badge--transcode{color:var(--method-transcode);background:var(--warning-soft)}
.pl-badge--danger{color:var(--danger);background:var(--danger-soft)}
.pl-badge--codec{font:500 11px/16px var(--font-mono);height:20px;padding:0 6px;background:none;border:1px solid var(--line-strong);color:var(--ink-muted)}

/* PosterCard */
.pl-poster{display:flex;flex-direction:column;gap:var(--space-2);width:100%;min-width:0;cursor:pointer}
.pl-poster__art{position:relative;aspect-ratio:2/3;border-radius:var(--radius-sm);background:var(--poster-placeholder);overflow:hidden;outline:1px solid var(--line);outline-offset:-1px}
.pl-poster__art img{width:100%;height:100%;object-fit:cover;display:block}
.pl-poster.is-focus .pl-poster__art{box-shadow:var(--shadow-glow);outline-color:var(--blue)}
.pl-poster__progress{position:absolute;left:var(--space-2);right:var(--space-2);bottom:var(--space-2);height:4px;border-radius:var(--radius-full);background:var(--scrim);overflow:hidden}
.pl-poster__progress span{display:block;height:100%;background:var(--blue);border-radius:var(--radius-full)}
.pl-poster__watched{position:absolute;top:var(--space-2);right:var(--space-2);width:22px;height:22px;border-radius:var(--radius-full);background:var(--blue);color:var(--on-blue);display:grid;place-items:center}
.pl-poster__watched svg{width:13px;height:13px}
.pl-poster__title{font:500 14px/18px var(--font-sans);color:var(--ink);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.pl-poster__meta{font:400 12px/16px var(--font-sans);color:var(--ink-subtle);margin-top:-6px}

/* SessionRow */
.pl-session{display:grid;grid-template-columns:40px minmax(0,1fr) auto;gap:var(--space-3);align-items:center;padding:var(--space-3) var(--space-4);border-bottom:1px solid var(--line)}
.pl-session__thumb{width:40px;height:60px;border-radius:var(--radius-xs);background:var(--poster-placeholder)}
.pl-session__title{font:500 14px/20px var(--font-sans);color:var(--ink);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.pl-session__who{font:400 13px/18px var(--font-sans);color:var(--ink-muted)}
.pl-session__tech{font:400 12px/16px var(--font-mono);color:var(--ink-subtle);margin-top:2px}
.pl-session__bar{height:3px;border-radius:var(--radius-full);background:var(--surface-3);margin-top:var(--space-2);overflow:hidden}
.pl-session__bar span{display:block;height:100%;background:var(--ink-muted)}
.pl-session__right{display:flex;flex-direction:column;align-items:flex-end;gap:6px}
.pl-session__rate{font:500 13px/18px var(--font-mono);color:var(--ink)}

/* Panel */
.pl-panel{background:var(--surface-1);border:1px solid var(--line-strong);border-radius:var(--radius-lg)}
.pl-eyebrow{font:600 11px/14px var(--font-sans);letter-spacing:.08em;text-transform:uppercase;color:var(--ink-subtle)}
```
