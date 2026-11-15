# Platea e Jellyfin: funzioni a confronto

Analisi del 14 novembre 2026 sulle funzioni di Jellyfin 10.10–10.11 (server e web), per decidere cosa manca a Platea.
Legenda: ✅ c'è · 🟡 in parte · ❌ manca · ➖ fuori dal perimetro di Platea (per ora).
La colonna **Priorità** indica l'ordine di lavoro: **A** subito, **B** dopo, **C** quando avanza tempo.

## Permessi per utente (impostati dall'amministratore)

| Funzione | Jellyfin | Platea | Priorità |
| --- | --- | --- | --- |
| Amministratore, account disattivato | ✅ | ✅ | — |
| Librerie accessibili | ✅ | ✅ | — |
| Riproduzione, remux, transcodifica consentiti | ✅ | ✅ | — |
| Bitrate massimo da remoto | ✅ | ✅ (limite per utente) | — |
| Sessioni contemporanee massime | ✅ | ✅ | — |
| Accesso da remoto | ✅ | ✅ | — |
| Classificazione massima | ✅ | ✅ (sigle di ogni paese convertite in età) | — |
| Bloccare i titoli senza classificazione | ✅ | ✅ | — |
| **Download del file originale** | ✅ (EnableContentDownloading) | ✅ (link firmato riprendibile, anche Infuse) | — |
| Scadenza dell'account | ❌ | ✅ (solo Platea) | — |
| Fasce orarie di accesso (AccessSchedules) | ✅ | ✅ | — |
| Tag consentiti / bloccati | ✅ | ❌ | C |
| Blocco dopo N tentativi di accesso falliti | ✅ | 🟡 (rallentamento, non blocco) | B |
| Gestione dei propri dispositivi | ✅ | ✅ (anche "scollega tutti gli altri") | — |
| Cancellare contenuti dal disco | ✅ | ➖ (Platea non scrive mai nelle cartelle dei media) | — |
| Gestire collezioni e sottotitoli | ✅ | ❌ | C |
| Controllo remoto di altre sessioni | ✅ | ❌ | C |
| Utente nascosto dalla schermata di accesso | ✅ | ➖ (Platea non elenca gli utenti all'accesso) | — |

## Preferenze dell'utente (scelte dall'utente)

| Funzione | Jellyfin | Platea | Priorità |
| --- | --- | --- | --- |
| Lingua dell'interfaccia | ✅ | ✅ (it, en) | — |
| **Lingua audio preferita** | ✅ | ✅ | — |
| **Lingua e modalità dei sottotitoli** (predefiniti, sempre, solo forzati, nessuno, intelligente) | ✅ | ✅ (sottotitoli di testo) | — |
| Ricordare le tracce scelte | ✅ | ✅ (per film e per serie, come lingue) | — |
| **Episodio successivo automatico** (con conto alla rovescia) | ✅ | ✅ (anche nelle playlist) | — |
| Salto avanti/indietro configurabile | ✅ | ✅ | — |
| Aspetto dei sottotitoli (dimensione, sfondo) | ✅ | ✅ | — |
| **Sezioni della home** (ordine, visibilità) e librerie nascoste dalla home | ✅ | ✅ | — |
| Nascondere i già visti tra gli "aggiunti di recente" | ✅ | ✅ | — |
| Immagine del profilo | ✅ | ✅ | — |
| Tema chiaro/scuro | ✅ | ✅ | — |
| Qualità massima predefinita | ✅ | 🟡 (ogni video parte in originale; un limite scelto nel player vale per quel video) | B |
| Cambio password | ✅ | ✅ | — |

## Libreria e navigazione

| Funzione | Jellyfin | Platea | Priorità |
| --- | --- | --- | --- |
| Continua a guardare, prossimi episodi, aggiunti di recente | ✅ | ✅ | — |
| Preferiti, visto/non visto, intere serie | ✅ | ✅ | — |
| Filtri per genere, lettera, visti, preferiti | ✅ | ✅ | — |
| Filtri per anno, risoluzione (4K), HDR | ✅ | ✅ (decennio, 4K/1080p/720p/SD, HDR) | — |
| **Pagina della persona** (filmografia) | ✅ | ✅ | — |
| **Titoli simili** | ✅ | ✅ (la saga per prima) | — |
| Pagine per genere e studio | ✅ | 🟡 (filtro per genere) | B |
| Collezioni | ✅ (manuali) | ✅ (automatiche da NFO e TMDB) | — |
| Playlist | ✅ | ✅ | — |
| Playlist condivise con altri utenti | ✅ (10.10) | ❌ | C |
| Versioni multiple dello stesso film | ✅ | ✅ | — |
| Trailer | ✅ | ❌ | C |
| Extra (dietro le quinte) | ✅ | ❌ (ignorati dallo scanner) | C |
| Episodi mancanti nelle stagioni | ✅ | ❌ | C |

## Riproduzione

| Funzione | Jellyfin | Platea | Priorità |
| --- | --- | --- | --- |
| Direct play, remux, transcodifica, motivo della scelta | ✅ | ✅ (con il motivo sempre visibile) | — |
| Transcodifica hardware con ripiego | ✅ | ✅ | — |
| HLS fMP4, HEVC copiato | ✅ | ✅ | — |
| Capitoli (navigazione) | ✅ | ✅ (tacche sulla barra e pannello) | — |
| Miniature sulla barra di avanzamento (trickplay) | ✅ | ✅ (per libreria, spente sulle remote) | — |
| Salto della sigla e dei titoli di coda | 🟡 (plugin, segmenti 10.10) | ❌ | C |
| Sottotitoli esterni e interni, conversione in WebVTT | ✅ | ✅ | — |
| Ricerca e download di sottotitoli (OpenSubtitles) | 🟡 (plugin) | ❌ | C |
| Visione di gruppo (SyncPlay) | ✅ | ❌ | C |
| Chromecast, DLNA | 🟡 | ➖ | — |

## Server e amministrazione

| Funzione | Jellyfin | Platea | Priorità |
| --- | --- | --- | --- |
| Dashboard, attività, log in tempo reale, lavori pianificati | ✅ | ✅ | — |
| Chiavi API | ✅ | ✅ (con ambito) | — |
| Backup e ripristino | ✅ (10.11) | ✅ (automatici e da file) | — |
| Notifiche dei file locali | ✅ | ✅ | — |
| NFO locali | ✅ | ✅ (+ .plexmatch) | — |
| Modifica dei metadati, identifica | ✅ | ✅ | — |
| Scelta o caricamento delle immagini | ✅ | 🟡 (caricamento sì; scelta tra le alternative di TMDB no) | C |
| Accesso con codice da un altro dispositivo (Quick Connect) | ✅ | ✅ (anche API Jellyfin) | — |
| Messaggio o avviso sulla pagina di accesso, CSS personalizzato | ✅ | ❌ | C |
| Webhook e notifiche esterne | 🟡 (plugin) | ❌ | C |
| TV in diretta, DVR, musica, libri, foto | ✅ | ➖ (fase 7) | — |

## Stato della priorità A (14 novembre 2026)

Fatta: tutte le voci qui sotto, salvo dove indicato.

## Cosa fare subito (priorità A)

1. **Download del file originale** con il permesso per utente, il pulsante nella pagina e il controllo di scadenza e di età.
2. **Preferenze di riproduzione**: lingue audio e sottotitoli, modalità sottotitoli, episodio successivo automatico con conto alla rovescia, salto configurabile, aspetto dei sottotitoli, tracce ricordate.
3. **Home personalizzabile**: sezioni e ordine, librerie nascoste, già visti nascosti tra gli aggiunti.
4. **Immagine del profilo**.
5. **Permessi in più**: fasce orarie di accesso, blocco dei titoli senza classificazione.
6. **Pagina della persona** con filmografia, **titoli simili** nel dettaglio.
7. **Grafica**: home con titolo in evidenza, dettaglio più ricco, caricamenti con segnaposto.
