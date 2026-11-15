# Platea: media server open source in Rust + FFmpeg — Piano di progetto

Nov 13, 2026 · @Enrico Chiarcossi

## Sintesi

Platea è fattibile e ha uno spazio preciso: un server Rust a processo unico che serve il file originale quando può, fa remux o transcodifica su GPU Intel, NVIDIA o AMD solo quando deve, e scansiona storage cloud senza aprire i file. La web UI va separata dal server, e più interfacce possono collegarsi allo stesso server tramite un'unica API pubblica.

| Decisione | Scelta | Motivo |
| --- | --- | --- |
| Web UI separata | Sì: repository e rilasci separati; il server ne incorpora comunque una copia | Più interfacce e app mobili senza toccare il server; l'installazione resta un solo eseguibile |
| Più web UI su un server | Sì: ogni interfaccia è un sito statico che punta all'indirizzo del server | Basta un'API pubblica con token e origini consentite |
| App Android e iOS | App native nella fase 6: Android in Kotlin, Apple in Swift con Liquid Glass. Prima: web UI responsive e Infuse | La web UI copre subito ogni dispositivo con un browser; Infuse copre i dispositivi Apple |
| Server | Rust, Tokio, axum, SQLite in modalità WAL | Un eseguibile, nessun servizio esterno, letture concorrenti |
| FFmpeg | Processo esterno sorvegliato; jellyfin-ffmpeg consigliato | Un crash non abbatte il server; driver GPU già inclusi |
| Riproduzione | Il client dichiara cosa regge; il server sceglie direct play, poi remux, poi transcodifica | Il metodo più economico vince sempre, con motivo registrato |
| Transcodifica hardware | Intel QSV e VA-API, NVIDIA NVENC, AMD VA-API e AMF | Prova reale all'avvio e ripiego automatico su software |
| Scansione | Tre fasi: elenco, identificazione dal nome, analisi in coda limitata | Zero file aperti in una scansione senza modifiche |
| Metadati | TMDB, con identificativo nel nome o ricerca con punteggio | Automatico, con elenco degli elementi dubbi |
| Configurazione | Tutto nel database, dashboard generata dallo schema delle impostazioni | Nessun file da modificare dopo il primo avvio |
| Licenza | AGPL-3.0 per server e web UI, proposta | Protegge il requisito "tutto illimitato" anche nei fork |
| Installazione | Docker Compose con server e web UI, poi procedura guidata nel browser | Un comando per avviare, nessun file da modificare |

Due avvertenze. Esistono già almeno quattro progetti Rust con lo stesso obiettivo di fondo: conviene provarli prima di scrivere codice. E il progetto è grande: la prima versione utile coincide con le fasi da 0 a 2 della roadmap, tutto il resto viene dopo.

## Obiettivi e principi

Il server riproduce il file originale ogni volta che il client lo regge e usa FFmpeg solo come ultima risorsa. Tutto il resto del progetto discende da questa regola e da quella gemella: non aprire mai un file remoto se la risposta è già nel database.

### Requisiti

| Requisito | Cosa significa in pratica | Come si verifica |
| --- | --- | --- |
| Direct play sempre | Il file viene servito così com'è, con richieste HTTP Range, senza passare da FFmpeg | CPU del server vicina a zero durante la riproduzione |
| Remux prima della transcodifica | Se il problema è solo il contenitore o l'audio, il video viene copiato senza ricodifica | Video in `-c:v copy` in tutti i casi in cui il codec è supportato |
| Transcodifica solo se inevitabile | Ricodifica del solo flusso non supportato, mai di tutti per default | Motivo della transcodifica registrato e visibile in dashboard |
| Transcodifica hardware | Intel (iGPU e Arc), NVIDIA e AMD, con ripiego automatico su software | Test di capacità all'avvio per ogni encoder e decoder |
| Account e amministratore | Utenti con permessi per libreria, uno o più admin | Nessuna funzione raggiungibile senza token valido |
| Nessuna licenza, nessun limite | Utenti, dispositivi, stream, librerie e funzioni illimitati, per sempre | Nessun controllo di licenza nel codice |
| Molti utenti senza blocchi | Nessuna operazione lenta sul percorso di una richiesta HTTP | Test di carico con centinaia di stream diretti contemporanei |
| Scansioni leggere | Una scansione legge solo l'elenco dei file, non il loro contenuto | Zero byte letti dai file già noti e non modificati |
| Associazione automatica TMDB | Film e serie riconosciuti dal nome di file e cartelle | Tasso di riconoscimento misurato su una libreria di prova |
| Tutto dalla dashboard | Nessun file di configurazione da modificare a mano dopo il primo avvio | Ogni impostazione ha un campo nell'interfaccia admin |
| Client separati | Web UI, app Android e iOS parlano tutti con la stessa API pubblica | La web UI ufficiale non usa endpoint privati |
| Open source | Codice, API e formato dei dati pubblici | Licenza libera nel repository dal primo commit |
| Web UI responsive | La stessa interfaccia si adatta a telefono, tablet e computer, comandi a tocco inclusi | Ogni pagina utilizzabile sullo schermo di un telefono |
| Avvio con Docker Compose | Un file avvia server e web UI; la configurazione iniziale avviene nel browser | Dal comando di avvio alla prima riproduzione senza modificare file |

### Fuori dalla prima versione

Queste funzioni restano fuori dalla v1 per tenere il nucleo piccolo e veloce. Lo schema del database le prevede, così aggiungerle dopo non richiede migrazioni distruttive.

- Live TV e registrazione
- Musica, libri, foto
- Sistema di plugin
- Download per visione offline e sincronizzazione
- DLNA e casting

### Principi di progettazione

1. Il server non tocca i byte del video se non è costretto.
2. Il database è la fonte di verità per la libreria: lo storage si interroga il meno possibile.
3. Ogni operazione lenta è un lavoro in coda, con concorrenza limitata e configurabile per libreria.
4. L'API è il prodotto. La web UI è un client come gli altri.
5. FFmpeg è un processo esterno, sorvegliato dal server e sostituibile.
6. Si misura prima di ottimizzare: ogni obiettivo di prestazione ha un numero e un test.

## Cosa imparare da Jellyfin ed Emby

Jellyfin dimostra che un server gratuito e senza limiti funziona; i suoi punti deboli storici sono il database e la scansione, non la riproduzione. Emby mostra cosa evitare nel modello di licenza. Entrambi condividono la stessa origine: Jellyfin è nato nel 2018 come fork di Emby quando Emby ha chiuso il codice.

### Confronto

| Aspetto | Jellyfin | Emby | Questo progetto |
| --- | --- | --- | --- |
| Licenza | Libera (GPL), gratis, nessun limite | Codice chiuso, funzioni avanzate con Emby Premiere | Libera, nessun controllo di licenza |
| Limiti | Nessuno | 30 dispositivi per le funzioni Premiere nella licenza standard, verificati online ([fonte](https://emby.media/support/articles/Premiere-Limits.html)) | Nessuno |
| Transcodifica hardware | Inclusa | Funzione Premiere | Inclusa |
| Runtime | C# su .NET 10 | C# su .NET | Rust, un solo eseguibile |
| Database | SQLite, riscritto nella 10.11 e ottimizzato nella 12.0 | SQLite | SQLite in modalità WAL, schema relazionale dal primo giorno |
| Decisione di riproduzione | Il client invia un profilo dispositivo, il server sceglie | Stesso modello | Stesso modello, con motivo della scelta registrato |
| Transcodifica | FFmpeg 8.1 con patch proprie (jellyfin-ffmpeg) | FFmpeg con build propria | FFmpeg esterno: jellyfin-ffmpeg consigliato, FFmpeg di sistema accettato |
| Client | Web in repository separato, molte app di terzi sull'API pubblica | App proprie | API pubblica più strato compatibile Jellyfin |
| Estensioni | Plugin .NET caricati nel server, da ricompilare a ogni versione maggiore | Plugin .NET | Nessun plugin nella v1; poi webhook e provider esterni |

Jellyfin 12.0 è uscito il 7 settembre 2026 ([note di rilascio](https://jellyfin.org/posts/jellyfin-release-12.0/)).

### Da copiare

1. Il profilo dispositivo. Il client dichiara contenitori, codec, livelli e bitrate che regge; il server confronta e decide. È il cuore del "direct play prima di tutto".
2. L'API pubblica come unico contratto. È ciò che ha permesso la nascita di decine di client di terzi.
3. Il web client in un repository separato dal server.
4. jellyfin-ffmpeg. È una build di FFmpeg già pronta con i driver e le patch per Intel, NVIDIA e AMD: conviene usarla invece di mantenerne una propria.
5. Le convenzioni sui nomi: `Film (Anno)/`, `Serie/Season 01/S01E02`, e l'identificativo nel nome della cartella, per esempio `[tmdbid-603]`. Chi arriva da Jellyfin o Emby non deve rinominare nulla.

### Da non ripetere

1. Dati serializzati dentro una colonna. Jellyfin teneva l'intero contenuto di una playlist dentro l'elemento playlist; per correggerlo sono servite due versioni maggiori, la 10.11 e la 12.0.
2. Manutenzione del database durante la scansione. Jellyfin l'ha separata solo nella 12.0.
3. Scanner lento per costruzione. Le note della 12.0 dicono che il lavoro ha riguardato la lettura della libreria, non lo scanner.
4. Migrazioni lunghe al primo avvio, con scansione completa obbligatoria dopo l'aggiornamento.
5. Plugin dentro il processo del server, che si rompono a ogni versione maggiore.
6. Funzioni dietro una licenza verificata online, come in Emby.

### Progetti Rust già esistenti

L'idea non è nuova: almeno quattro progetti riscrivono un server compatibile con i client Jellyfin in Rust. Prima di scrivere codice conviene provarli, per capire se contribuire a uno di essi o ripartire da zero. Le descrizioni vengono dai rispettivi README e non sono state verificate.

| Progetto | Cosa dichiara |
| --- | --- |
| [Rustyfin](https://github.com/apophisnow/rustyfin) | Implementa l'API REST di Jellyfin; stack Axum, Tokio, SQLx |
| [Pelagos](https://github.com/sguzman/pelagos) | Pre-alpha; punta a sostituire per intero Jellyfin Server, inclusi i client per smart TV |
| [jellyfin-rs](https://github.com/dydydd/jellyfin-rs) | Leggero, centrato sullo streaming diretto; transcodifica e HLS non ancora presenti |
| [Remux](https://github.com/lostb1t/remux) | API compatibile Jellyfin, sorgenti locali, WebDAV e add-on Stremio; licenza AGPL-3.0 |

Lo spazio che resta libero è quello dei requisiti di questo piano: scansione pensata per storage cloud, catena completa direct play, remux e transcodifica hardware, e una dashboard admin che copre ogni impostazione.

## Architettura generale

Un solo processo Rust contiene API, libreria, sessioni di riproduzione e coda dei lavori. FFmpeg e ffprobe girano come processi figli; tutti i client stanno fuori e parlano HTTP.

&#91;embedded content: architettura · 3 tipi di client, 5 moduli, 4 risorse esterne\]

Tutto entra dall'API pubblica. Verso l'esterno il server legge lo storage, avvia FFmpeg, interroga TMDB e scrive sul proprio disco.

### Moduli del server

| Modulo | Responsabilità |
| --- | --- |
| `api` | Route REST e WebSocket, autenticazione, permessi, documento OpenAPI |
| `stream` | Direct play: lettura del file e risposta alle richieste HTTP Range |
| `playback` | Confronto tra file e profilo del client, scelta del metodo, sessioni |
| `transcode` | Costruzione dei comandi FFmpeg, backend hardware, segmenti HLS, sorveglianza dei processi |
| `scanner` | Elenco delle cartelle, confronto con il database, lettura dei nomi |
| `probe` | Analisi tecnica dei file con ffprobe, in coda e con limiti |
| `metadata` | TMDB, download e ridimensionamento delle immagini, cache |
| `jobs` | Coda persistente con priorità, limiti di concorrenza e pianificazione |
| `store` | Database, migrazioni, tutte le query |
| `config` | Impostazioni tipizzate salvate nel database, applicate a caldo |
| `events` | Bus interno: notifica ai client di libreria aggiornata, sessioni, lavori |

### Perché un solo processo

Un processo unico si installa copiando un file e non ha latenza tra componenti. I moduli sono crate separati con confini netti, quindi spostare la transcodifica su altre macchine in futuro significa avviare lo stesso eseguibile in modalità worker, non riscrivere.

### Percorso di una riproduzione

1. Il client chiama `POST /api/v1/playback/info` con l'identificativo dell'elemento e il proprio profilo dispositivo.
2. Il server legge dal database i dati tecnici del file, già raccolti dall'analisi. Lo storage non viene toccato.
3. Il modulo `playback` risponde con il metodo scelto e l'URL: file diretto oppure playlist HLS.
4. Direct play: il client richiede `/stream/{id}` con Range e riceve i byte del file originale.
5. Remux o transcodifica: il client richiede la playlist; FFmpeg parte alla richiesta del primo segmento.
6. Il client invia la posizione ogni dieci secondi. Il server aggiorna lo stato di visione e tiene viva la sessione.

### Dove sta lo stato

| Directory | Contenuto | Si può cancellare |
| --- | --- | --- |
| `data/` | Database e file di avvio | No: va nel backup |
| `cache/` | Immagini ridimensionate, sottotitoli estratti | Sì: si rigenera |
| `transcode/` | Segmenti temporanei delle sessioni attive | Sì: meglio su SSD o in RAM |

## Stack tecnologico

Rust stabile con Tokio e axum per il server, SQLite per i dati, FFmpeg come processo esterno, TypeScript e React per la web UI. Sono tutte scelte mature: nessuna richiede di scommettere su una libreria giovane.

### Server

| Area | Scelta | Motivo |
| --- | --- | --- |
| Runtime asincrono | Tokio | Standard di fatto; le letture da file system vanno su pool di thread dedicati |
| HTTP | axum con tower-http | Range, compressione, tracing e limiti come middleware componibili; HTTP/2 incluso |
| Database | SQLite in modalità WAL, tramite sqlx | Un file, nessun servizio da amministrare, letture concorrenti |
| Migrazioni | sqlx, incorporate nell'eseguibile | Aggiornare significa sostituire un file |
| Ricerca | SQLite FTS5 | Ricerca per titolo senza servizi esterni |
| Documento API | utoipa | OpenAPI generato dal codice, sempre allineato |
| Password | Argon2id | Standard attuale per l'hashing delle password |
| Token | Casuali, salvati come hash nel database | Revocabili uno per uno, a differenza dei JWT |
| Cache in memoria | moka | Cache concorrente con scadenza e limite di dimensione |
| Coda lavori | Tabella SQLite e worker Tokio | Persistente, nessun broker esterno |
| FFmpeg | Processo figlio con `tokio::process` | Un crash di FFmpeg non fa cadere il server |
| Analisi file | ffprobe con uscita JSON | Stesso parser di FFmpeg, nessuna sorpresa |
| Client HTTP | reqwest con limitatore di frequenza | Rispetto dei limiti di TMDB |
| Immagini | crate `image` e `fast_image_resize` | Ridimensionamento su richiesta, poi cache su disco |
| Osservabilità | tracing, metriche in formato Prometheus | Ogni obiettivo di prestazione è misurabile |
| Notifiche file | notify per dischi locali | Sui mount di rete non funziona: lì si usa la scansione pianificata |
| UI incorporata | rust-embed | La web UI ufficiale viaggia dentro l'eseguibile |

### Scelte scartate

| Alternativa | Perché no |
| --- | --- |
| Binding diretti a libav (FFmpeg come libreria) | Build complessa su ogni piattaforma, un errore di memoria in un decoder abbatte il server, vincoli di licenza più stretti |
| PostgreSQL nella v1 | Obbliga a installare e amministrare un servizio; resta possibile in seguito dietro lo stesso modulo `store` |
| JWT per le sessioni | Non si revocano senza una lista nera, che riporta lo stato sul server |
| Microservizi | Più latenza e più installazione, nessun vantaggio su una sola macchina |
| Broker di code esterno | Una dipendenza in più per un carico che SQLite regge |

### Client

| Client | Tecnologia |
| --- | --- |
| Web UI e dashboard admin | TypeScript, React, Vite, TanStack Query, hls.js; responsive, progettata prima per il telefono; client API generato dal documento OpenAPI |
| Android, Android TV e Fire TV | Kotlin, Jetpack Compose e Compose for TV, Media3 (ExoPlayer) |
| iPhone, iPad e Mac | Swift, SwiftUI con Liquid Glass, da iOS, iPadOS e macOS 26.1; AVPlayer più un motore per i formati che non apre |

### Distribuzione

- Un file Docker Compose avvia due container, server e web UI: è il modo di installazione consigliato.
- Immagini Docker per amd64 e arm64; quella del server contiene jellyfin-ffmpeg.
- Eseguibili statici per Linux; poi Windows e macOS.
- Nessuna dipendenza a runtime oltre a FFmpeg e ai driver della GPU.

## Server e client separati

Sì, la web UI va separata dal server, e più interfacce possono collegarsi allo stesso server. La condizione è che l'API pubblica sia l'unico punto di contatto: la UI ufficiale non deve usare nulla che un client di terzi non possa usare.

### Cosa si guadagna e cosa costa

| Vantaggio | Costo |
| --- | --- |
| Web UI, app Android e app iOS usano la stessa API: nessuna funzione va scritta due volte sul server | L'API va versionata e mantenuta compatibile |
| Server e interfaccia si rilasciano con tempi diversi | Serve una tabella di compatibilità tra versioni |
| Chiunque può scrivere un'interfaccia alternativa | Vanno gestiti CORS e origini consentite |
| L'API è costretta a essere completa e documentata | Più lavoro iniziale di progettazione |

### Come farlo

1. Repository separati: `server` e `web`; più avanti `android` e `ios`.
2. Il server incorpora una copia della web UI ufficiale e la serve su `/`. L'installazione resta un solo eseguibile. Un'opzione permette di disattivarla o di indicare un'altra cartella. Nell'installazione con Docker Compose la web UI gira in un container separato.
3. Una web UI alternativa è un sito statico ospitato ovunque. Al primo avvio chiede l'indirizzo del server, come fanno le app mobili.
4. API sotto `/api/v1`, descritta da un documento OpenAPI pubblicato a ogni rilascio. Un endpoint deprecato resta attivo per un'intera versione maggiore.
5. Autenticazione con token nell'intestazione `Authorization`, non con cookie: funziona allo stesso modo da browser, app e script.
6. Elenco delle origini consentite (CORS) configurabile dalla dashboard.
7. Un canale WebSocket per gli eventi: libreria aggiornata, stato delle sessioni, avanzamento dei lavori, comandi remoti al player.
8. Un endpoint pubblico con nome, identificativo e versione del server, per il collegamento iniziale dei client.

### La dashboard admin

La dashboard fa parte della web UI ufficiale ma è un pacchetto a sé, caricato solo per gli amministratori. Usa la stessa API pubblica con permessi admin. Chi scrive un'interfaccia alternativa per gli utenti non deve rifare l'amministrazione.

### Web UI responsive

La web UI è una sola, progettata prima per lo schermo del telefono e poi allargata a tablet e computer. Finché non esistono le app native è il client per Android, e resta il client per chiunque non voglia installare nulla.

- La disposizione segue la larghezza: una colonna su telefono, griglia su schermi grandi.
- I comandi del player sono pensati per il tocco: doppio tocco per saltare, trascinamento sulla barra, schermo intero.
- Si installa nella schermata Home come app web, con icona e avvio a tutto schermo.
- Anche la dashboard admin si usa da telefono.
- Interfaccia multilingua: italiano e inglese dalla prima versione, con un sistema di traduzione scalabile
  (un dizionario per lingua, chiavi tipizzate, formati di numeri, date e durate secondo la lingua).
  Aggiungere una lingua significa aggiungere un file, senza toccare i componenti. I codici motivo restano in inglese.

Un limite da conoscere: il browser di un telefono riproduce meno formati di un'app nativa, quindi remux e transcodifica scattano più spesso. È il motivo principale per fare le app native.

### Infuse: lo strato compatibile Jellyfin

Prima delle app native, i dispositivi Apple sono coperti da Infuse. Platea espone, accanto all'API nativa, la piccola parte dell'API di Jellyfin che Infuse usa: l'app si collega come a un server Jellyfin, senza modifiche.

Decisione: lo strato compatibile serve solo a Infuse. L'app ufficiale Jellyfin per Android è esclusa, perché richiederebbe il client web di Jellyfin e quasi tutta la sua API. L'API nativa resta il contratto principale; lo strato compatibile è un crate separato, scritto in modo indipendente e senza copiare codice di Jellyfin.

Cosa deve offrire Platea a Infuse, che si collega con un login diretto al server ([fonte](https://support.firecore.com/hc/articles/360006462093)):

- login con utente e password di Platea;
- elenco delle librerie, elenchi e dettagli di film, serie, stagioni ed episodi;
- immagini;
- ricerca;
- stato di visione e posizione di ripresa;
- streaming del file originale con richieste Range.

Infuse si adatta bene al progetto. Decodifica da sé quasi tutti i formati, quindi chiede il file originale e il server resta in direct play. Va usato in modalità diretta, quella predefinita: per la modalità libreria Infuse consiglia un plugin del server, InfuseSync, che su Platea non esiste.

Due vincoli. Infuse legge nome e versione del server al collegamento, quindi lo strato compatibile deve presentarsi come una versione di Jellyfin che Infuse accetta. E un aggiornamento di Infuse può cambiare le chiamate che fa: servono test che confrontano le risposte di Platea con quelle di un Jellyfin vero.

### App native

Le app native arrivano nella fase 6, dopo la 1.0. Usano la stessa API pubblica della web UI e hanno un player proprio, che riproduce in direct play più formati di un browser.

| Piattaforma | Linguaggio e interfaccia | Dispositivi | Player |
| --- | --- | --- | --- |
| Android | Kotlin con Jetpack Compose; Compose for TV per l'uso con il telecomando | Telefoni, tablet, Android TV, Fire TV con Fire OS | Media3 (ExoPlayer) |
| Apple | Swift con SwiftUI e Liquid Glass; versione minima 26.1 di iOS, iPadOS e macOS | iPhone, iPad, Mac | AVPlayer, più un motore aggiuntivo per i formati che AVPlayer non apre |

Tre punti da sapere:

1. Liquid Glass. Con la 26.1 come versione minima non servono percorsi alternativi per i sistemi precedenti. Barre di navigazione, barre degli strumenti e schede standard adottano il nuovo aspetto da sole; ai controlli su misura, come quelli del player, si applica con le API dedicate di SwiftUI ([sessione Apple sul nuovo design](https://developer.apple.com/videos/play/wwdc2025/284/?time=835)).
2. Fire TV. L'app Android funziona sui modelli con Fire OS, che è basato su Android. I modelli con Vega OS, cioè Fire TV Stick 4K Select e gli Stick più recenti, non eseguono app Android e richiedono un'app scritta apposta ([fonte](https://www.aftvnews.com/?p=45426)). Sono fuori dal piano.
3. MKV su Apple. AVPlayer apre MP4, MOV e HLS ma non MKV. Per il direct play di MKV serve un motore aggiuntivo, per esempio basato su mpv o VLC; senza, il server fa un remux, che costa poco ma non è direct play. La scelta è tra le decisioni aperte.

Apple TV non è tra le piattaforme richieste: lì resta Infuse.

### Ordine dei client

| Client | Quando |
| --- | --- |
| Web UI responsive con dashboard admin, anche da telefono | Dalla fase 1 |
| Infuse su iPhone, iPad, Apple TV e Mac | Fase 2 |
| App Android in Kotlin: telefono, tablet, Android TV, Fire TV | Fase 6 |
| App Apple in Swift: iPhone, iPad, Mac | Fase 6 |

## Motore di riproduzione

Ogni richiesta passa da una funzione pura che confronta il file con il profilo del client e restituisce il metodo meno costoso tra direct play, remux e transcodifica. La funzione non fa I/O: lavora solo su dati già nel database, quindi risponde in millisecondi e si testa con tabelle di casi.

&#91;embedded content: decisione di riproduzione · 3 verifiche, 4 esiti\]

Le verifiche si fanno in quest'ordine. La prima risposta negativa decide il metodo e resta registrata come motivo.

### I tre metodi

| Metodo | Cosa fa il server | Costo | Quando |
| --- | --- | --- | --- |
| Direct play | Serve il file originale con HTTP Range | Solo lettura e rete | Contenitore, video, audio e sottotitoli sono tutti supportati dal client |
| Remux | FFmpeg cambia contenitore copiando il video; l'audio è copiato o convertito | CPU minima | Contenitore non supportato, oppure solo l'audio non supportato |
| Transcodifica | FFmpeg ricodifica il video, su GPU se disponibile | GPU o CPU | Codec video non supportato, bitrate oltre il limite, sottotitoli da imprimere, HDR su schermo SDR |

### Il profilo dispositivo

Il client dichiara ciò che sa riprodurre e il server non indovina mai. Il profilo contiene:

- contenitori accettati;
- codec video con profili, livelli, profondità di bit e risoluzione massima;
- codec audio con numero massimo di canali;
- formati di sottotitoli che sa disegnare da solo, testo e immagine;
- formati HDR supportati;
- bitrate massimo, scelto dall'utente o misurato;
- protocolli: file diretto, HLS.

La web UI costruisce il profilo interrogando il browser con le API Media Capabilities. Le app native lo costruiscono dai decoder del dispositivo.

### Ordine delle verifiche

1. Se l'elemento ha più versioni, si preferisce quella riproducibile in direct play.
2. Video: codec, profilo, livello, profondità di bit, risoluzione e HDR sono supportati? Se no, transcodifica video.
3. Il bitrate totale rientra nel limite? Se no, transcodifica video a bitrate inferiore.
4. Audio: la traccia scelta è supportata? Se no, conversione del solo audio con video copiato.
5. Sottotitoli di testo: consegnati a parte in WebVTT, senza toccare il video. Sottotitoli a immagine: passati al client se li sa disegnare, altrimenti impressi nel video.
6. Contenitore supportato? Se no, remux.
7. Tutto supportato: direct play.

Ogni risposta negativa produce un codice motivo, per esempio `VideoCodecNotSupported` o `ContainerNotSupported`. Il motivo viene salvato nella sessione e mostrato in dashboard: l'amministratore vede sempre perché un file non è andato in direct play.

L'amministratore può vietare remux o transcodifica per utente. In quel caso il client riceve un errore chiaro invece di un caricamento infinito.

### Direct play

- Endpoint `GET /stream/{id}` con Range, risposte 206, ETag calcolato da dimensione e data di modifica.
- Lettura a blocchi con contropressione: in memoria c'è solo il blocco in transito, mai il file.
- Il descrittore del file resta aperto per la durata della sessione. Un browser fa molte richieste Range sullo stesso file e riaprirlo ogni volta su un mount cloud costa secondi.
- Limite di banda per utente opzionale.
- FFmpeg non viene mai avviato.

### Remux e transcodifica con HLS

- Uscita in HLS con segmenti fMP4 di pochi secondi, durata configurabile.
- La playlist è generata dal server in anticipo, con tutti i segmenti elencati. La durata è nota dall'analisi, quindi la barra di avanzamento è completa dal primo istante.
- FFmpeg parte dal segmento richiesto. Un salto fuori dalla finestra già prodotta ferma il processo e lo riavvia alla nuova posizione.
- Nel remux i confini dei segmenti devono cadere sui keyframe. L'indice dei keyframe si legge dalle tabelle del contenitore durante l'analisi, senza leggere il video.
- Rallentamento: FFmpeg viene sospeso quando è avanti di un certo margine rispetto al punto di visione, e ripreso quando il margine si riduce.
- I segmenti già visti vengono cancellati; la sessione termina allo stop o dopo un periodo senza richieste.
- Un semaforo limita le transcodifiche contemporanee, separatamente per hardware e software. I direct play non hanno limiti.

### Transcodifica hardware

Intel, NVIDIA e AMD sono supportati dalla prima versione della transcodifica, con rilevamento automatico e ripiego su software. La tabella segue le interfacce che Jellyfin considera validate ([documentazione](https://jellyfin.org/docs/general/post-install/transcoding/hardware-acceleration)).

| Produttore | Linux | Windows | Da sapere |
| --- | --- | --- | --- |
| Intel, iGPU e Arc | QSV; VA-API per le GPU più vecchie | QSV | Nessun limite di sessioni contemporanee |
| NVIDIA | NVDEC e NVENC | NVDEC e NVENC | Le schede consumer hanno un limite di sessioni di codifica imposto dal driver |
| AMD | VA-API | AMF | Nessun limite di sessioni; su Linux AMF è sconsigliata |
| Nessuna GPU | Codifica software | Codifica software | Sempre disponibile come ripiego |

Come è costruito:

1. Rilevamento. All'avvio e su richiesta il server codifica un secondo di video sintetico per ogni combinazione di encoder e dispositivo. Elencare gli encoder compilati in FFmpeg non basta: conta solo ciò che funziona davvero su quella macchina.
2. Matrice delle capacità. Il risultato, cioè quali codec si decodificano e si codificano e con quale profondità di bit, è salvato e mostrato in dashboard.
3. Un'interfaccia comune. Ogni backend (QSV, VA-API, NVENC, AMF, software) fornisce inizializzazione del dispositivo, decoder, filtri ed encoder. Il comando FFmpeg nasce da una funzione pura, verificata con test che confrontano gli argomenti prodotti.
4. Catena in GPU. Decodifica, ridimensionamento, tone mapping e codifica restano in memoria video quando il formato lo permette.
5. Ripiego parziale. Se la GPU non decodifica un formato, si decodifica in software e si codifica in hardware. È il caso dell'H.264 a 10 bit, che nessuna GPU Intel, NVIDIA o AMD decodifica.
6. Ripiego totale. Se FFmpeg hardware fallisce nei primi secondi, la sessione riparte in software e l'evento viene registrato. Dopo più fallimenti la combinazione viene esclusa.
7. Scelta del dispositivo. Con più GPU l'amministratore indica quale usare; l'assegnazione automatica alla meno carica arriva dopo.
8. Codec di uscita. H.264 per compatibilità universale; HEVC o AV1 quando il client li dichiara e la GPU li codifica.
9. Tone mapping da HDR a SDR solo quando il client non supporta l'HDR, eseguito in GPU.
10. Docker. La documentazione spiega come passare al container `/dev/dri` per Intel e AMD e il runtime NVIDIA.

Ordine di sviluppo consigliato: Intel per prima, perché le iGPU sono le più diffuse nei server domestici; poi NVIDIA; poi AMD.

## Libreria e scansione leggera

Una scansione costa un elenco per cartella e zero byte letti dai file già noti. L'analisi tecnica dei file nuovi è un lavoro separato, lento per scelta e con un tetto configurabile.

### Perché conta sui mount cloud

Su un mount rclone elencare una cartella è economico, perché rclone tiene in memoria l'elenco. Aprire un file invece avvia un download, consuma quota delle API del provider e può richiedere secondi. Un server che apre ogni file a ogni scansione rende la libreria inutilizzabile e rischia blocchi temporanei da parte del provider.

### Tre fasi indipendenti

| Fase | Cosa fa | Accesso allo storage | Quando |
| --- | --- | --- | --- |
| 1. Elenco | Legge nome, dimensione e data di modifica di ogni voce e li confronta con il database | Solo elenco delle cartelle | A ogni scansione |
| 2. Identificazione | Ricava film, serie, stagione ed episodio dal nome; cerca su TMDB | Nessuno | Solo per i file nuovi |
| 3. Analisi | Legge le intestazioni del file con ffprobe: codec, tracce, durata, keyframe | Pochi megabyte per file | Una volta per file, in coda |

Dopo le fasi 1 e 2 l'elemento è già in libreria con locandina e trama, prima che il file sia mai stato aperto. Se l'utente lo riproduce prima dell'analisi, il file viene analizzato in quel momento con priorità massima.

L'identità di un file è la terna percorso, dimensione, data di modifica. Finché non cambia, il risultato dell'analisi vale per sempre.

### Rilevare i cambiamenti

| Storage | Metodo |
| --- | --- |
| Dischi locali | Notifiche del sistema operativo, più una scansione periodica di sicurezza |
| rclone e altri mount di rete | Scansione pianificata: le notifiche del sistema non vedono i cambi fatti da remoto |
| rclone con controllo remoto attivo | Il server chiede a rclone di aggiornare l'elenco di una cartella (`vfs/refresh`) prima di leggerla |
| Strumenti esterni | Un endpoint riceve la cartella cambiata e scansiona solo quella |

L'ultima riga è la più efficace sulle librerie grandi. Lo strumento che aggiunge un file avvisa il server e la scansione tocca una sola cartella.

### Cosa non parte mai da solo su una libreria remota

- Estrazione di immagini dal video: anteprime dei capitoli e miniature per la barra di avanzamento.
- Calcolo di hash sul contenuto.
- Estrazione dei sottotitoli incorporati: avviene al primo utilizzo, poi resta in cache.
- Analisi dell'audio per riconoscere le sigle.

Ogni voce è un'opzione per libreria. Due profili predefiniti, Locale e Remoto, impostano valori sensati; l'amministratore li modifica dalla dashboard.

### Limiti dell'analisi

- Concorrenza per libreria: una o due analisi alla volta sulle librerie remote, di più sui dischi locali.
- Tetto di analisi al minuto, per restare sotto le quote del provider.
- Fasce orarie consentite.
- Pausa automatica quando ci sono riproduzioni attive, opzionale.
- ffprobe con dimensione di sonda e durata di analisi ridotte: bastano le intestazioni.

### Robustezza

1. Mount assente. Se la radice di una libreria è vuota o irraggiungibile, la scansione si ferma con un avviso e non cancella nulla. È l'errore classico: il mount cade, la libreria viene svuotata e tutto va riassociato.
2. File scomparsi. Un file mancante viene segnato come non disponibile e rimosso solo dopo un periodo di grazia configurabile. Lo stato di visione degli utenti si conserva.
3. Spostamenti e rinomine. Un file che riappare altrove con stessa dimensione e data viene riconosciuto: conserva analisi, metadati e stato di visione.
4. Mount bloccati. Ogni operazione su file ha un timeout e gira su un pool di thread dedicato alla libreria. Un mount fermo non blocca le altre librerie né l'API.
5. Scritture raggruppate. Una transazione per cartella, non una per file.

### Lettura dei nomi

- Film: `Titolo (Anno)`, con etichette di qualità ed edizione ignorate o usate come nome della versione.
- Serie: `S01E02`, `1x02`, episodi multipli `S01E01-E02`, episodi per data, numerazione assoluta per gli anime.
- Identificativi espliciti nella cartella o nel file: `[tmdbid-603]`, `[imdbid-tt0133093]`.
- Più versioni dello stesso titolo nella stessa cartella: un solo elemento con più file.
- Cartelle di contenuti extra riconosciute e tenute fuori dall'elenco principale.

Il parser si sviluppa contro una raccolta di migliaia di nomi reali con il risultato atteso. Ogni nome letto male diventa un nuovo caso di test.

## Metadati e The Movie Database

L'associazione è automatica in due passi: identificativo esplicito nel nome se c'è, altrimenti ricerca per titolo e anno con un punteggio. Ogni risposta di TMDB resta in cache nel database, così un elemento già associato non genera più richieste.

### Come avviene l'associazione

1. Identificativo esplicito. Se cartella o file contengono `[tmdbid-…]`, `[imdbid-…]` o `[tvdbid-…]`, l'associazione è certa e non parte nessuna ricerca.
2. Ricerca. Titolo e anno ricavati dal nome vanno alla ricerca di TMDB per film o per serie.
3. Punteggio. Ogni candidato riceve un voto: somiglianza del titolo normalizzato, anche con titolo originale e titoli alternativi; anno con tolleranza di uno; popolarità come spareggio.
4. Soglia. Sopra la soglia l'associazione è automatica. Sotto, l'elemento finisce nell'elenco "da verificare" della dashboard con i candidati pronti da scegliere.
5. Dettagli. Una sola richiesta per film porta con sé cast, immagini, identificativi esterni e classificazioni per età. Per le serie: una richiesta per la serie e una per stagione.
6. Immagini. Locandina, sfondo e logo si scaricano una volta e si ridimensionano in locale. Le foto degli attori si scaricano solo quando una pagina le mostra.

### Correzioni manuali

- "Identifica": l'amministratore cerca e sceglie la scheda giusta.
- Blocco dei campi: un campo modificato a mano non viene sovrascritto dagli aggiornamenti.
- Associazione rifiutata: il candidato scartato non viene riproposto.

### Aggiornamenti

- TMDB espone l'elenco delle schede modificate di recente. Il server aggiorna solo gli elementi presenti in quell'elenco, non tutta la libreria.
- Le serie in corso si controllano con cadenza più stretta, per i nuovi episodi.
- Lingua dei metadati e paese delle classificazioni si scelgono per libreria, con ripiego sull'inglese.

### Limiti e condizioni d'uso di TMDB

| Punto | Stato | Conseguenza per il progetto |
| --- | --- | --- |
| Costo | Gratuita per uso non commerciale con attribuzione | Un server gratuito e open source rientra; un uso commerciale richiede un accordo con TMDB |
| Attribuzione | Logo TMDB e indicazione della fonte | Pagina "Informazioni" in ogni client, con la dicitura che il prodotto non è approvato da TMDB |
| Limite di richieste | Il vecchio limite di 40 richieste ogni 10 secondi è disattivato dal dicembre 2019; resta una protezione intorno a 50 richieste al secondo e 20 connessioni per indirizzo IP ([fonte](https://www.themoviedb.org/talk/67c4e964c7caba424c91dba0)) | Limitatore lato server con valori prudenti e configurabili, più attesa crescente in caso di risposta 429 |
| Chiave API | Personale, legata a un account | L'amministratore inserisce la propria chiave in dashboard; se distribuirne una con il progetto è una questione aperta |

### Predisposizione per altre fonti

TMDB è l'unica fonte nella v1, ma il codice passa da un'interfaccia `MetadataProvider`. Aggiungere TVDB, file NFO locali o una fonte per gli anime significa scrivere un nuovo provider, senza toccare scanner e database.

## Utenti, amministratore e sicurezza

Due ruoli, amministratore e utente, più permessi impostabili utente per utente. Ogni accesso è un token revocabile legato a un dispositivo, e nessun endpoint riceve mai un percorso di file dal client.

### Account

- Al primo avvio una procedura guidata crea il primo amministratore. Completata, l'endpoint di configurazione iniziale si disattiva in modo permanente.
- Più amministratori possibili. L'ultimo amministratore non si può eliminare né declassare.
- Nomi utente unici senza distinzione tra maiuscole e minuscole.
- Password con hash Argon2id e lunghezza minima configurabile.
- Numero di utenti illimitato.

### Permessi per utente

| Permesso | Valore predefinito |
| --- | --- |
| Librerie visibili | Tutte |
| Remux consentito | Sì |
| Transcodifica video consentita | Sì |
| Bitrate massimo da remoto | Nessun limite |
| Riproduzioni contemporanee | Illimitate |
| Classificazione per età massima | Nessuna |
| Accesso da fuori rete locale | Sì |
| Gestione dei propri dispositivi | Sì |
| Scadenza dell'account | Nessuna. Dopo la data l'utente resta attivo e sfoglia tutto, ma non riproduce né scarica (codice `AccountExpired`); le riproduzioni in corso si chiudono |

I limiti esistono come strumenti per l'amministratore, non come restrizioni del prodotto: di serie è tutto aperto.

### Sessioni e token

- Il login restituisce un token casuale di 256 bit. Nel database ne resta solo l'hash.
- Ogni token registra utente, dispositivo, nome e versione del client, ultimo utilizzo e indirizzo IP.
- L'utente vede i propri dispositivi e li scollega. L'amministratore vede e revoca quelli di tutti.
- Per gli URL di streaming si usa un token breve, valido solo per quella sessione di riproduzione. I player che non possono inviare intestazioni lo ricevono nell'URL, e il token principale non finisce mai nei log.
- Chiavi API separate per le integrazioni, con ambito limitato: per esempio solo "avvia scansione".

### Difese

1. Limite ai tentativi di login per indirizzo IP e per utente, con attesa crescente.
2. Solo identificativi negli endpoint. I percorsi vengono dal database e si verifica che ricadano sotto la radice di una libreria.
3. Argomenti di FFmpeg passati come lista, mai come stringa a una shell. Nessun testo dell'utente entra in un comando.
4. I segreti, come la chiave TMDB, si possono scrivere ma l'API non li restituisce.
5. Registro delle attività: accessi, modifiche alle impostazioni, gestione utenti.
6. Nessuna telemetria. Le uniche connessioni in uscita sono verso TMDB e, se attivato, il controllo degli aggiornamenti.

### Rete

- Il server parla HTTP e sta dietro un reverse proxy che gestisce TLS. È la configurazione che anche Jellyfin raccomanda.
- Elenco dei proxy fidati configurabile, per leggere correttamente l'indirizzo del client.
- TLS integrato può arrivare dopo, come opzione.

### Più avanti

Login con codice breve per le TV, autenticazione esterna tramite OIDC o LDAP, profili con PIN. Il login passa da un'interfaccia `AuthProvider`, così queste aggiunte non toccano il resto.

## Dashboard admin

Ogni impostazione vive nel database ed è raggiungibile dalla dashboard. Il file di avvio contiene solo ciò che serve prima che il database sia aperto.

### Cosa resta fuori dalla dashboard

Quattro valori, passati come argomenti, variabili d'ambiente o file: indirizzo e porta di ascolto, directory dei dati, directory di cache e di transcodifica, livello di log iniziale. Nient'altro.

### Come sono fatte le impostazioni

1. Ogni gruppo di impostazioni è una struttura Rust tipizzata, con valori predefiniti e regole di validazione.
2. Dalla struttura si genera uno schema JSON. La dashboard disegna i moduli a partire dallo schema: una nuova impostazione compare nell'interfaccia senza scrivere codice di interfaccia.
3. Le modifiche si applicano a caldo. Le poche che richiedono un riavvio sono segnalate come tali.
4. Ogni modifica finisce nel registro delle attività, con autore e valore precedente.
5. Esportazione e importazione dell'intera configurazione in un file, per backup e migrazioni.

### Pagine

| Pagina | Contenuto |
| --- | --- |
| Panoramica | Riproduzioni attive con metodo e motivo, uso di CPU, GPU e memoria, banda, lavori in corso, avvisi |
| Librerie | Percorsi, tipo, profilo Locale o Remoto, pianificazione delle scansioni, limiti dell'analisi, estrazioni opzionali, lingua; scansione di una libreria o di una sola cartella |
| Utenti | Creazione, disattivazione, permessi, librerie visibili, dispositivi e sessioni, revoca dei token |
| Riproduzione | Regole predefinite su remux e transcodifica, bitrate massimo da remoto, durata dei segmenti, margine di rallentamento |
| Transcodifica | Percorso di FFmpeg, backend e dispositivo, matrice delle capacità rilevate, codec abilitati, qualità, numero massimo di transcodifiche, tone mapping, pulsante "prova ora" |
| Metadati | Chiave TMDB, lingua e paese, soglia di associazione automatica, elementi da verificare, aggiornamento |
| Rete | URL pubblico, proxy fidati, origini consentite, limiti di banda |
| Lavori | Coda, cronologia, errori, nuovo tentativo, pianificazioni |
| Log e attività | Log in tempo reale con filtro per livello, registro delle attività |
| Sistema | Versione, backup e ripristino del database, esportazione della configurazione, chiavi API |

### Due dettagli che fanno la differenza

- La pagina Transcodifica mostra ciò che la macchina sa fare davvero, codec per codec, con il risultato dell'ultima prova. L'amministratore non deve indovinare quali caselle spuntare.
- La Panoramica mostra per ogni riproduzione il motivo per cui non è in direct play. È lo strumento per portare la libreria verso lo zero transcodifiche.

## Installazione e primo avvio

Un file Docker Compose avvia server e web UI. Aprendo il browser parte una procedura guidata che crea l'amministratore e porta alla prima riproduzione, senza modificare alcun file.

### Il file Compose

È un esempio di come sarà: nomi delle immagini e variabili sono provvisori finché non esiste il primo rilascio.

```yaml
services:
  server:
    image: registro.example/platea-server:latest
    restart: unless-stopped
    ports:
      - "8096:8096"
    environment:
      PLATEA_DATA_DIR: /data
      PLATEA_CACHE_DIR: /cache
      PLATEA_TRANSCODE_DIR: /transcode
    volumes:
      - ./data:/data
      - ./cache:/cache
      - ./transcode:/transcode
      - /mnt/media:/media:ro,rslave
    # GPU Intel o AMD: togliere il commento
    # devices:
    #   - /dev/dri:/dev/dri
    # GPU NVIDIA: togliere il commento
    # deploy:
    #   resources:
    #     reservations:
    #       devices:
    #         - driver: nvidia
    #           count: all
    #           capabilities: [gpu]

  web:
    image: registro.example/platea-web:latest
    restart: unless-stopped
    depends_on:
      - server
    ports:
      - "8080:80"
    environment:
      PLATEA_SERVER_URL: http://server:8096
```

| Voce | A cosa serve |
| --- | --- |
| `server`, porta 8096 | API per la web UI, per Infuse e per le app native. È la porta predefinita di Jellyfin, comoda per Infuse |
| `web`, porta 8080 | La web UI: file statici dietro un piccolo server web che inoltra le chiamate API al server |
| `./data` | Database e configurazione: è la cartella da salvare nel backup |
| `./cache` e `./transcode` | Dati che si rigenerano |
| `/mnt/media` | La libreria, in sola lettura. L'opzione `rslave` fa vedere al container un mount rclone montato dopo l'avvio |
| `devices` e `deploy` | Accesso alla GPU per la transcodifica hardware |

### Procedura guidata

1. `docker compose up -d` avvia i due container.
2. Il server scrive nei log un codice di primo avvio. Impedisce che qualcun altro sulla rete configuri il server prima del proprietario.
3. Aprendo la web UI sulla porta 8080 parte la procedura guidata, che chiede il codice.
4. Creazione dell'account amministratore.
5. Prima libreria: tipo, cartella scelta sfogliando ciò che il container vede sotto `/media`, profilo Locale o Remoto.
6. Chiave TMDB e lingua dei metadati.
7. Transcodifica: il server prova la GPU e mostra che cosa funziona.
8. Fine. La scansione parte e la home si riempie mentre arrivano i metadati. Platea è già utilizzabile dal browser, anche da telefono.

Ogni passo dopo il quarto si può saltare e completare più tardi dalla dashboard.

### Dopo il primo avvio

- Infuse si collega all'indirizzo del server sulla porta 8096, con utente e password di Platea.
- Per aggiornare: `docker compose pull`, poi `docker compose up -d`. Le migrazioni del database partono da sole, dopo la copia di sicurezza.
- Per l'accesso da fuori casa serve un reverse proxy con TLS davanti alle due porte.

## Modello dati

Tabelle normalizzate, il file fisico separato dall'elemento logico, e nessun dato su cui si filtra o si ordina nascosto dentro una colonna JSON. È la lezione diretta delle due riscritture del database di Jellyfin.

### Tabelle principali

| Tabella | Contenuto |
| --- | --- |
| `users` | Nome, hash della password, ruolo, permessi, stato |
| `user_library_access` | Quali librerie vede ogni utente |
| `tokens` | Hash del token, utente, dispositivo, client, versione, ultimo utilizzo, IP |
| `api_keys` | Chiavi per le integrazioni con il loro ambito |
| `libraries` | Nome, tipo, profilo di scansione, lingua dei metadati |
| `library_paths` | Le cartelle radice di ogni libreria |
| `items` | Elementi logici: film, serie, stagione, episodio, collezione; titolo, anno, numeri d'ordine, trama, durata, voto, date |
| `media_files` | File fisici: percorso relativo, dimensione, data di modifica, contenitore, durata, bitrate, stato, etichetta della versione |
| `media_streams` | Tracce di ogni file: tipo, codec, profilo, livello, profondità di bit, risoluzione, HDR, canali, lingua, predefinita o forzata |
| `keyframes` | Indice dei keyframe per file, usato dal remux |
| `external_ids` | Identificativi TMDB, IMDb, TVDB per elemento |
| `genres`, `people`, `studios` | Anagrafiche, con tabelle di collegamento verso gli elementi |
| `images` | Tipo, origine, percorso locale, dimensioni, colore segnaposto |
| `user_item_data` | Per utente ed elemento: posizione, visto, numero di visioni, ultima visione, preferito, tracce scelte |
| `collections`, `playlists` | Con una riga per ogni elemento contenuto |
| `jobs` | Tipo, dati, priorità, stato, tentativi, prossima esecuzione, errore |
| `settings` | Un gruppo di impostazioni per riga |
| `playback_history` | Utente, elemento, client, metodo, motivo, durata: la base delle statistiche |
| `activity_log` | Accessi e modifiche amministrative |
| `metadata_cache` | Risposte dei provider per chiave e lingua |

### Scelte di schema

1. Elemento e file sono due tabelle. Un film in 1080p e in 4K è un solo elemento con due file. Vale anche per gli episodi, cosa che Jellyfin ha ottenuto solo con la 12.0.
2. Lo stato di visione è legato all'elemento, non al file. Sostituire un file con una versione migliore non azzera nulla.
3. Chiavi intere all'interno, identificativi opachi nell'API. Le query restano veloci e gli identificativi pubblici non rivelano l'ordine di inserimento.
4. Percorsi relativi alla radice della libreria. Spostare una libreria su un altro mount significa cambiare una riga.
5. I file mancanti restano con uno stato e una data, per il periodo di grazia.
6. Ricerca a testo pieno con una tabella FTS5 allineata a `items`.

### Indici essenziali

| Query | Indice |
| --- | --- |
| Sfoglia una libreria in ordine alfabetico | `items (library_id, kind, sort_title)` |
| Stagioni ed episodi di una serie | `items (parent_id, index_number)` |
| Confronto durante la scansione | `media_files (library_path_id, relative_path)` unico |
| Continua a guardare | `user_item_data (user_id, last_played)` |
| Aggiunti di recente | `items (library_id, added_at)` |
| Prossimo lavoro in coda | `jobs (state, priority, run_after)` |

### Migrazioni

Piccole, solo in avanti, incorporate nell'eseguibile. Prima di ognuna il server copia il file del database. Una migrazione che richiede più di qualche secondo su una libreria grande va riprogettata come lavoro in background.

## Prestazioni e scalabilità

Il server regge molti utenti se nessuna richiesta HTTP resta in attesa di un disco lento, di una scrittura sul database o di FFmpeg. Con il direct play come caso normale, il carico è quasi tutto lettura e rete, non CPU.

### Regole

1. Niente di bloccante sul runtime asincrono. Le operazioni su file vanno su pool di thread dedicati, uno per libreria e di dimensione limitata. Un mount rclone lento esaurisce il proprio pool e non tocca il resto.
2. Una sola connessione scrive sul database, alimentata da una coda. Le scritture piccole e frequenti, come la posizione di riproduzione, si accumulano e partono insieme in una transazione.
3. Le letture usano un pool di connessioni separato e non aspettano mai lo scrittore: è ciò che la modalità WAL di SQLite garantisce.
4. Tutti gli elenchi sono paginati con cursore. Nessun endpoint restituisce l'intera libreria.
5. Ogni risorsa scarsa ha un semaforo: analisi dei file, transcodifiche, download di immagini, richieste a TMDB. Quando è piena, il lavoro aspetta in coda invece di moltiplicare i thread.
6. Priorità fisse: riproduzione, poi richieste interattive, poi metadati, poi analisi, poi manutenzione.
7. Cache in memoria per ciò che si legge a ogni richiesta: impostazioni, permessi, dati tecnici dei file in riproduzione.
8. Immagini ridimensionate una volta, salvate su disco e servite con cache lunga nel browser.
9. Gli eventi viaggiano su WebSocket. Nessun client interroga il server a intervalli.
10. Memoria limitata per riproduzione: un solo blocco in transito, qualunque sia la dimensione del file.

### Obiettivi proposti

Sono traguardi da confermare con i benchmark su una macchina di riferimento, non misure.

| Misura | Obiettivo |
| --- | --- |
| File aperti in una scansione senza modifiche | 0 |
| Scansione senza modifiche di 100.000 file su disco locale | Meno di 60 secondi |
| Avvio del server con 50.000 elementi | Meno di 2 secondi |
| Memoria a riposo | Meno di 100 MB |
| Pagina di libreria da 50 elementi, 95° percentile | Meno di 50 ms |
| Decisione di riproduzione, 95° percentile | Meno di 20 ms |
| Primo byte in direct play da disco locale | Meno di 100 ms |
| Primo fotogramma con transcodifica hardware | Meno di 3 secondi |
| Direct play contemporanei su una CPU a 4 core | 500, con limite dato da rete e dischi |

### Come si verifica

- Test di carico che simulano centinaia di player con richieste Range e salti casuali.
- Un file system di prova con latenza artificiale, per riprodurre un mount cloud lento o bloccato.
- Una libreria sintetica da 100.000 file vuoti con nomi realistici, per scanner e database.
- Profilazione con tokio-console e flamegraph a ogni fase della roadmap.
- Gli endpoint principali hanno un test di prestazione nella CI: un peggioramento blocca la modifica.

### Oltre una macchina

Non serve nella v1, ma il disegno lo permette. La transcodifica può spostarsi su altre macchine avviando lo stesso eseguibile in modalità worker. Il modulo `store` può ricevere un secondo backend PostgreSQL per chi vuole più istanze sullo stesso database.

## Open source

La licenza va scelta prima del primo commit pubblico, perché cambiarla dopo richiede il consenso di tutti i contributori. La proposta è AGPL-3.0 per il server, senza accordo di cessione dei diritti: è la combinazione che rende più difficile chiudere il progetto in futuro. Queste sono considerazioni generali, non un parere legale.

### Licenze a confronto

| Licenza | Effetto | A favore | Contro |
| --- | --- | --- | --- |
| AGPL-3.0 | Chi modifica il server e lo offre in rete deve pubblicare le modifiche | Nessuno può farne una versione chiusa o a pagamento | Alcune aziende la evitano per politica interna |
| GPL-3.0 | L'obbligo scatta solo se si distribuisce il programma | Copyleft noto e diffuso | Un servizio ospitato può tenere chiuse le modifiche |
| MIT o Apache-2.0 | Nessun obbligo di condivisione | Massima adozione e libertà di riuso | Chiunque può creare una versione chiusa |

### Proposta per componente

| Componente | Licenza proposta | Motivo |
| --- | --- | --- |
| Server | AGPL-3.0 | Protegge il requisito "nessuna licenza, tutto illimitato" anche nei fork |
| Web UI | AGPL-3.0 | Stesso motivo |
| Documento API e client generati | Apache-2.0 o MIT | Chiunque può scrivere un client con la licenza che preferisce |
| App iOS e Android | Da valutare, per esempio MPL-2.0 | Le licenze GPL creano attriti con le condizioni degli store di app |

Due note. FFmpeg è un programma separato avviato dal server, quindi la sua licenza non si estende al codice del progetto; distribuirlo nell'immagine Docker comporta però il rispetto della sua licenza. Niente accordo di cessione dei diritti: basta la firma dei commit che attesta l'origine del contributo.

### Struttura del repository del server

| Crate | Contenuto |
| --- | --- |
| `core` | Tipi di dominio ed errori condivisi |
| `store` | Database, migrazioni, query |
| `config` | Impostazioni tipizzate e schema |
| `scanner` | Elenco, confronto, lettura dei nomi |
| `probe` | ffprobe e lettura delle intestazioni |
| `metadata` | Provider TMDB e immagini |
| `playback` | Profili, decisione, sessioni |
| `transcode` | Backend hardware, comandi FFmpeg, HLS |
| `jobs` | Coda e pianificazione |
| `api` | Route, WebSocket, OpenAPI |
| `compat-jellyfin` | Strato di compatibilità, opzionale |
| `server` | L'eseguibile: avvio, collegamento dei moduli, UI incorporata |

### Qualità e rilasci

- CI a ogni modifica: formattazione, clippy senza avvisi, test, controllo di licenze e vulnerabilità delle dipendenze.
- Test di integrazione con un FFmpeg vero su clip campione brevi, una per combinazione di contenitore e codec.
- Test hardware su macchine dedicate con una GPU Intel, una NVIDIA e una AMD, anche offerte dalla comunità.
- Versioni semantiche, registro delle modifiche, note di aggiornamento a ogni rilascio.
- Immagine Docker e binari pubblicati automaticamente da un tag.

### Comunità

- Dal primo giorno: README con obiettivi e non-obiettivi, guida per contribuire, codice di condotta, modelli per le segnalazioni.
- Roadmap pubblica, la stessa di questo documento.
- Le modifiche all'API passano da una proposta scritta e discussa prima del codice.
- Una politica per la segnalazione riservata delle vulnerabilità.
- Sito di documentazione con installazione, GPU, reverse proxy e configurazione di rclone.

## Roadmap

Otto fasi, ognuna con un criterio di uscita verificabile. Già alla fine della fase 1 Platea si avvia con Docker Compose e si usa dal browser; la versione 1.0 esce alla fine della fase 5 e le app native arrivano nella fase 6. Le date dipendono da quante persone lavorano al progetto e non sono indicate.

&#91;embedded content: roadmap · 7 fasi, 6 criteri di uscita\]

Ogni rombo è un criterio di uscita: la fase successiva parte quando è soddisfatto. Il rombo evidenziato è il rilascio della 1.0.

### Fase 0 — Fondamenta

- [x] Workspace Rust con i crate vuoti e la CI attiva
- [x] Avvio del server, file di avvio, log strutturati
- [x] Database SQLite, migrazioni, scrittore unico e pool di lettura
- [x] Procedura di primo avvio e creazione dell'amministratore
- [x] Login, token, middleware di autenticazione e permessi
- [x] Documento OpenAPI generato e pubblicato
- [x] Scheletro della web UI con login

Criterio di uscita: login completo via API e via web UI, CI verde.

### Fase 1 — Libreria e direct play

- [x] Librerie e percorsi, con profilo Locale e Remoto
- [x] Scanner: elenco e confronto senza aprire file
- [x] Lettura dei nomi di film e serie, con la raccolta di casi di test
- [x] Coda dei lavori con priorità e limiti
- [x] Analisi con ffprobe in coda limitata
- [x] Associazione TMDB, immagini, elenco "da verificare"
- [x] API per sfogliare, cercare e vedere i dettagli
- [x] Endpoint di streaming con Range
- [x] Stato di visione, "continua a guardare", "aggiunti di recente"
- [x] Web UI responsive: home, librerie, dettaglio, player in direct play, anche da telefono
- [x] Pagine admin minime: librerie, utenti, lavori
- [x] Procedura guidata di primo avvio nella web UI
- [x] Immagini Docker e file Docker Compose con server e web UI
- [x] Protezione da mount assente e periodo di grazia per i file mancanti

Criterio di uscita: dopo `docker compose up` e la procedura guidata, un film su un mount rclone si riproduce in direct play nel browser, anche da telefono. Una scansione senza modifiche apre zero file.

### Fase 2 — Remux e transcodifica software

- [x] Profilo dispositivo generato dalla web UI
- [x] Funzione di decisione con codici motivo e tabella di test
- [x] Sessioni di riproduzione e sorveglianza di FFmpeg
- [x] HLS con segmenti fMP4 e playlist generata dal server
- [x] Remux con indice dei keyframe
- [x] Conversione del solo audio
- [x] Transcodifica video software
- [x] Sottotitoli: testo in WebVTT, immagine impressa nel video
- [x] Rallentamento, pulizia dei segmenti, limite di sessioni
- [x] Permessi per utente su remux e transcodifica
- [x] Strato compatibile Jellyfin per Infuse (provato con richieste simulate; manca la prova con l'app reale)

Criterio di uscita: una raccolta di clip campione si riproduce in Chrome, Firefox e Safari, ognuna con il metodo meno costoso previsto. Infuse sfoglia la libreria e riproduce in direct play.

### Fase 3 — Transcodifica hardware

- [x] Rilevamento con prova reale e matrice delle capacità (verificato solo senza GPU)
- [x] Interfaccia comune dei backend e test sugli argomenti di FFmpeg
- [ ] Intel: QSV e VA-API
- [ ] NVIDIA: NVDEC e NVENC
- [ ] AMD: VA-API su Linux, AMF su Windows
- [x] Ripiego parziale e totale su software
- [ ] Tone mapping da HDR a SDR in GPU
- [ ] Uscita HEVC e AV1 quando il client li dichiara
- [ ] Guida Docker per le tre famiglie di GPU

Criterio di uscita: la raccolta di clip supera i test su una GPU Intel, una NVIDIA e una AMD, e il ripiego su software scatta da solo quando la GPU fallisce.

### Fase 4 — Dashboard completa

- [x] Impostazioni tipizzate con schema e moduli generati
- [x] Tutte le pagine elencate nella sezione Dashboard admin
- [x] Panoramica in tempo reale con metodo e motivo di ogni riproduzione
- [x] Log in tempo reale e registro delle attività
- [x] Backup, ripristino, esportazione della configurazione
- [x] Chiavi API ed endpoint di scansione mirata per strumenti esterni
- [x] Integrazione con il controllo remoto di rclone

Criterio di uscita: un'installazione nuova si configura per intero dal browser, senza modificare alcun file.

### Fase 5 — Prestazioni, robustezza e 1.0

- [x] Test di carico e file system con latenza artificiale
- [ ] Raggiungimento degli obiettivi della sezione Prestazioni
- [ ] Revisione di sicurezza esterna
- [ ] Documentazione completa per installazione, GPU, proxy e rclone
- [x] Immagine Docker su ghcr.io, amd64 e arm64 (binari per Linux, Windows e macOS rimandati: per ora solo Docker, decisione del 15/11/2026)
- [x] Procedura di aggiornamento provata tra versioni

Criterio di uscita: obiettivi di prestazione raggiunti e nessun problema di sicurezza aperto. Rilascio della 1.0.

### Fase 6 — App native

- [x] Client API per Kotlin e Swift generati dal documento OpenAPI (`scripts/generate-clients.sh`)
- [ ] App Android in Kotlin: login, librerie, ricerca, dettaglio, player Media3
- [ ] Interfaccia da telecomando per Android TV e Fire TV con Fire OS
- [ ] App Apple in Swift con Liquid Glass, da iOS 26.1: iPhone, iPad e Mac
- [ ] Player Apple: AVPlayer più il motore per i formati che non apre
- [ ] Profilo dispositivo costruito dai decoder reali di ogni dispositivo
- [ ] Pubblicazione su Google Play, Amazon Appstore e App Store

Criterio di uscita: le due app sono pubblicate e riproducono in direct play i formati che il browser dello stesso dispositivo non regge.

### Fase 7 — Ecosistema

- [ ] Miniature sulla barra di avanzamento e salto delle sigle, come opzioni
- [x] Collezioni automatiche e playlist
- [ ] Download per la visione offline nelle app
- [ ] Autenticazione esterna con OIDC o LDAP
- [x] Transcodifica su macchine worker (nodi: `docs/nodi.md`, anche copie delle cartelle con bilanciamento e failover)
- [ ] Musica e altri tipi di libreria

Criterio di uscita: nessuno, è lavoro continuo ordinato secondo le richieste degli utenti.

## Rischi e questioni aperte

Il rischio principale è l'ampiezza: Jellyfin è il risultato di anni di lavoro di molte persone, e un server completo con interfaccia e app è un progetto grande. La difesa è una prima versione stretta, cioè le fasi da 0 a 2, e il riuso di ciò che esiste.

### Rischi

| Rischio | Effetto | Contromisura |
| --- | --- | --- |
| Ampiezza del progetto | Il lavoro si ferma prima di essere utilizzabile | Fasi piccole e ognuna utilizzabile; valutare il contributo a un progetto Rust esistente; web UI responsive e Infuse prima delle app native |
| Varietà di client e codec | Ogni browser e ogni TV si comporta in modo diverso | Profilo dichiarato dal client, raccolta di clip campione, motivo registrato per ogni scelta |
| Driver GPU e versioni di FFmpeg | Transcodifica hardware instabile su alcune macchine | jellyfin-ffmpeg, prova reale all'avvio, ripiego automatico su software |
| Compatibilità con l'API Jellyfin | Superficie ampia e in movimento | Sottoinsieme limitato a ciò che usa Infuse, test che confrontano le risposte |
| Quote e blocchi dello storage cloud | Scansioni lente, sospensioni temporanee dell'account | Fasi separate, tetto alle analisi, timeout, nessuna cancellazione con mount assente |
| Condizioni d'uso di TMDB | Metadati non disponibili se cambiano | Chiave per installazione, cache locale, provider sostituibile |
| Un solo scrittore su SQLite | Attese in scrittura con moltissime sessioni | Scrittore unico con raggruppamento; PostgreSQL come opzione futura |
| Server esposto su Internet | Accessi non autorizzati | Le difese della sezione Sicurezza, revisione esterna prima della 1.0 |
| App native su due piattaforme | Tre interfacce da mantenere: web, Android e Apple | App solo dopo la 1.0, stessa API pubblica, client generati dal documento OpenAPI |

### Decisioni da prendere

- [ ] Partire da zero, oppure contribuire a uno dei progetti Rust esistenti o derivarne uno
- [x] Nome del progetto

  Platea. Restano da verificare marchi registrati e domini.
- [x] Strato compatibile Jellyfin

  Solo per Infuse, nella fase 2. Niente app ufficiale Jellyfin per Android.
- [x] Client per telefono e TV

  Web UI responsive dalla fase 1. App native nella fase 6: Android in Kotlin, Apple in Swift con Liquid Glass da iOS 26.1.
- [x] Installazione

  Docker Compose con server e web UI, e procedura guidata nel browser.
- [x] Licenza: AGPL-3.0-only
- [ ] Motore per il direct play di MKV nelle app Apple: mpv, VLC, oppure remux dal server
- [ ] Fire TV con Vega OS: fuori dal piano, oppure un'app dedicata in futuro
- [ ] Chiave TMDB: una per installazione oppure una distribuita con il progetto
- [ ] Solo SQLite, oppure anche PostgreSQL
- [x] Sistemi supportati al lancio: solo Docker (decisione del 15/11/2026; binari nativi rimandati)
- [ ] Tipi di libreria nella v1: solo film e serie, come proposto
- [ ] Quante persone e quante ore a settimana: da qui dipendono le date della roadmap

## Fonti

Pagine lette per intero:

- [Jellyfin 12.0, note di rilascio](https://jellyfin.org/posts/jellyfin-release-12.0/), 7 settembre 2026
- [Jellyfin per Android 2.7](https://jellyfin.org/posts/android-v2.7/), 2 agosto 2026
- [Emby, limiti di Premiere](https://emby.media/support/articles/Premiere-Limits.html)
- [TMDB, discussione sui limiti di richieste e sull'attribuzione](https://www.themoviedb.org/talk/67c4e964c7caba424c91dba0)

Pagine consultate dai risultati di ricerca:

- [Jellyfin, accelerazione hardware](https://jellyfin.org/docs/general/post-install/transcoding/hardware-acceleration), con le guide per [Intel](https://jellyfin.org/docs/general/administration/hardware-acceleration/intel) e [AMD](https://jellyfin.org/docs/general/post-install/transcoding/hardware-acceleration/amd)
- [Infuse, streaming da Plex, Emby e Jellyfin](https://support.firecore.com/hc/articles/360006462093)
- [Apple, sessione WWDC25 sul nuovo design](https://developer.apple.com/videos/play/wwdc2025/284/?time=835)
- [AFTVnews, Vega OS e le app Android su Fire TV](https://www.aftvnews.com/?p=45426)
- Repository di [Rustyfin](https://github.com/apophisnow/rustyfin), [Pelagos](https://github.com/sguzman/pelagos), [jellyfin-rs](https://github.com/dydydd/jellyfin-rs) e [Remux](https://github.com/lostb1t/remux)
- Documentazione di rclone su mount e controllo remoto, da copie pubbliche del repository

Tutto il resto, cioè architettura, schema, obiettivi di prestazione e roadmap, è una proposta di progetto e non proviene da fonti esterne.
