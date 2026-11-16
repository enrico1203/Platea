# Nodi

Un nodo è un'altra macchina con Platea avviato in modalità nodo (`platea node`, stessa immagine Docker del
server). Il server lo raggiunge in LAN o con Tailscale e gli affida due tipi di lavoro, anche insieme:

1. **Transcodifica.** Il nodo riceve il file dal server, lo transcodifica (con la sua GPU, se ne ha una) e rimanda i
   segmenti HLS. Il client parla sempre e solo con il server.
2. **Copie dei file (CDN).** Il nodo ha in copia alcune cartelle delle librerie (una copia vera o un secondo mount
   rclone). Direct play, download e app leggono il file da lì, con bilanciamento e ripiego tra le copie.
3. **Tutte e due.** Se il nodo ha la cartella, transcodifica leggendo il file in locale: dal server non passa niente.

Il nodo non ha database né configurazione propria: tutto si imposta e si controlla dal server, in
Amministrazione › Nodi.

## Installare un nodo

Sulla macchina del nodo, con [`docker-compose.node.yml`](../docker-compose.node.yml):

```sh
mkdir node-data && sudo chown 1000:1000 node-data
docker compose -f docker-compose.node.yml up -d
docker compose -f docker-compose.node.yml exec node platea node key   # chiave da dare al server
```

Poi sul server: Amministrazione › Nodi › Aggiungi nodo, con l'indirizzo (es. `100.64.0.2`, la porta 8097 si
aggiunge da sola) e la chiave. Il nodo diventa "Online" in pochi secondi.

La porta 8097 va tenuta sulla LAN o su Tailscale. Con `PLATEA_NODE_ALLOW` (es. `100.64.0.0/10`) il nodo accetta solo
le reti indicate. Senza la chiave giusta risponde 401 a tutto tranne `/node/v1/health`.

### GPU del nodo (es. Intel Arc in un LXC)

Nel compose del nodo: `devices: - /dev/dri:/dev/dri` e `group_add` con il gruppo che possiede
`/dev/dri/renderD128` (`stat -c %g /dev/dri/renderD128`). In un LXC Proxmox il dispositivo va prima passato al
container (cgroup `c 226:* rwm` e mount di `/dev/dri`). Poi, nella scheda del nodo: Prova GPU › Prova ora. La matrice
mostra cosa funziona davvero (codifica e decodifica H.264, HEVC, HEVC 10 bit, AV1). Backend, dispositivo, limiti,
preset e CRF del nodo si scelgono nel suo dialogo Modifica e arrivano al nodo da soli.

### Cartelle in copia

Montare le cartelle nel nodo in sola lettura sotto `/media` (o le radici di `PLATEA_NODE_MEDIA`), con la stessa
struttura della cartella sul server. Nel dialogo del nodo, per ogni cartella di libreria, si sceglie la sua copia
(pulsante Sfoglia). Il server controlla che alcuni file abbiano la stessa dimensione; prima di usare un file lo
ricontrolla (risultato tenuto 10 minuti). Una copia diversa o assente non si usa: il file arriva da un'altra copia.

La scansione resta sul server: il server deve vedere tutte le librerie.

### File ricevuti dal server

Un file che il nodo non ha in copia lo riceve dal server (`/api/v1/node-source/<token>`). Il nodo deve poter
raggiungere il server: il server lo prova da solo (ogni 5 minuti, ogni 15 secondi finché non va) e lo mostra nella
scheda del nodo ("Il nodo legge da … raggiungibile/non raggiungibile"). Se il nodo non raggiunge il server, in
automatico il file glielo manda il server (vedi sotto); con "Il nodo legge dal server" il nodo non riceve
transcodifiche di file che non ha in copia, così la riproduzione non aspetta il failover. Dietro un router di
sottorete Tailscale (o un NAT) l'indirizzo automatico è quello del router: in quel caso va scritto a mano in
Impostazioni › Nodi › Indirizzo del server per i nodi (es. `http://100.x.y.z:8096`), oppure si lascia che sia il
server a mandare il file.

Chi muove il file (dialogo del nodo › File ricevuti dal server › Chi muove il file):

| Scelta | Cosa fa |
| --- | --- |
| Automatico (predefinito) | Il nodo legge dal server se la prova riesce; altrimenti il server gli manda il file |
| Il nodo legge dal server | Solo il nodo che scarica: se non raggiunge il server, niente transcodifiche di file senza copia |
| Il server manda al nodo | Il server manda i blocchi sul collegamento con cui comanda il nodo (`/node/v1/push/<sessione>/…`): il nodo dice quali blocchi gli servono (attesa lunga), il server li legge dal suo disco e li carica. Funziona con il nodo dietro NAT o con Tailscale solo sul router. Sempre con la cache, mai in diretta |

Come il nodo legge quei file:

| Lettura | Cosa fa |
| --- | --- |
| A blocchi, in anticipo (predefinita) | Blocchi da 8 MiB, 4 in parallelo, fino a 1024 MB davanti a dove legge FFmpeg. Scarica solo quello che si guarda; assorbe rallentamenti della rete e del cloud |
| Copia completa | Tutto il file, a partire dal punto in cui si guarda (poi il resto). Salti istantanei, ma scarica anche quello che non si guarda |
| In diretta | FFmpeg legge dal server al ritmo della transcodifica, senza cache (comportamento delle versioni fino alla 0.1.2) |

FFmpeg legge dalla cache attraverso `http://127.0.0.1:8097/node/v1/local-source/<token>` (solo dalla macchina del
nodo); un blocco non ancora arrivato si scarica per primo. I blocchi restano nella cache del nodo (`/data/sources`, cioè `node-data/sources`; `PLATEA_NODE_SOURCE_DIR` per un altro disco;
tetto predefinito 50 GB, sempre almeno 10 GB liberi (un decimo del disco se è più piccolo di 100 GB), si toglie il file usato meno di recente): riprendere un film o
tornare indietro non riscarica. La cache si svuota al riavvio del nodo. Con poco spazio la copia completa diventa
lettura anticipata, e senza spazio si legge in diretta (in push la sessione va a un altro esecutore).

## Bilanciamento

"Questo server" compare tra i nodi con gli stessi campi, quindi si può decidere anche che il server transcodifichi
solo quando i nodi non possono.

- **Priorità**: 1 è la prima scelta. I livelli successivi lavorano solo quando nessuno del livello precedente può.
- **Peso**: tra chi ha la stessa priorità il lavoro si divide in proporzione. 90 e 10 → 90% e 10% delle sessioni.
  A parità completa (stessa priorità, stesso peso, stesso carico) vince un nodo sul server: con server e nodo alla
  pari la prima sessione va al nodo, poi si alternano.
  Peso 0 = mai.
- **Massimo contemporanee**: oltre il limite il nodo è "pieno" e il lavoro va avanti nell'elenco.
- **Accetta solo**: risoluzione della sorgente (solo 4K, fino a 1080p, personalizzata), HDR o SDR, codec della
  sorgente, librerie, transcodifiche e/o remux, "solo se la GPU regge" (decodifica della sorgente e codifica
  dell'uscita in GPU).
- **Svuota**: il nodo finisce le riproduzioni in corso e non ne prende di nuove (manutenzione).

Impostazioni › Nodi:

| Impostazione | Effetto |
| --- | --- |
| Bilanciamento | Per peso (predefinito), il meno carico, in ordine (il primo finché ha posto) |
| Preferisci chi ha il file in locale | A parità di priorità vince il nodo che ha il file: niente traffico dal server |
| Ripiego su questo server | Se nessuno accetta il lavoro transcodifica il server; spento, la riproduzione viene rifiutata (503) |
| Indirizzo del server per i nodi | Come il nodo legge i file dal server. Vuoto = l'IP da cui il nodo vede arrivare il server + la porta |
| Consegna diretta dai nodi | Direct play e download con un link firmato verso i nodi con un indirizzo pubblico |

Esempi:

- *Nodo GPU per quasi tutto, server di riserva*: nodo priorità 1, server priorità 2.
- *90% al nodo, 10% al server*: entrambi priorità 1, pesi 90 e 10.
- *Solo i 4K al nodo*: nodo con "Solo 4K", tutti priorità 1. Gli altri file li fa il server.

Un remux va su un nodo solo se il nodo ha il file in locale (altrimenti farebbe solo da passacarte).

## Failover

- Il server controlla ogni nodo ogni 5 secondi. Dopo 3 controlli falliti il nodo è offline e non riceve lavoro;
  quando torna, rientra da solo e riceve di nuovo la configurazione.
- Se l'esecutore scelto non accetta la sessione, si prova il successivo dell'elenco.
- Se il nodo cade a metà film, alla richiesta del segmento successivo la sessione riparte su un altro esecutore dallo
  stesso segmento: il player non se ne accorge (provato: 2 secondi). Eccezione: una transcodifica con uscita HEVC o
  AV1 (fMP4) cambierebbe l'init del flusso, quindi la sessione finisce (410) e il player web la riapre da solo alla
  posizione corrente.
- 3 lavori falliti in 5 minuti → il nodo resta fuori 2 minuti ("In pausa per errori"), poi riprova.
- Un nodo con una versione del protocollo diversa è "Da aggiornare" e non riceve lavoro.
- Se il server sparisce, dopo 60 secondi senza controlli il nodo chiude tutte le sue transcodifiche.
- Una copia che non risponde durante il direct play: la richiesta va alla copia successiva prima di mandare un byte.

Offline e ritorno online finiscono nell'attività e nei webhook (eventi `node.offline`, `node.online`, con
"Problemi" attivo nelle notifiche).

## Dalla dashboard

Per ogni nodo: stato, latenza, versione, GPU in uso, transcodifiche e letture in corso, slot GPU/CPU, carico, RAM,
spazio per i segmenti, cartelle in copia. Azioni: Modifica, Prova collegamento, Prova GPU, Velocità, Log, Svuota/Riprendi,
Ferma riproduzioni, Riavvia, Elimina. Nel pannello delle riproduzioni: "Eseguita da" e "Letta da".

**Velocità** misura la banda con dati casuali, 5 secondi per misura (circa 15 secondi in tutto, una prova alla volta
per nodo):

- *Server → nodo*: il server manda dati al nodo (come quando gli manda il file da transcodificare);
- *Nodo → server*: il server scarica dal nodo (segmenti transcodificati, file delle copie);
- *Il nodo legge dal server*: il nodo scarica dall'indirizzo del server con cui legge i file (token monouso su
  `/api/v1/node-speed/`). Se qui c'è un errore, quell'indirizzo non va: vedi Impostazioni › Nodi.

L'ultimo risultato resta nella scheda del nodo fino al riavvio del server. Un 4K in remux vuole circa 80–100 Mbit/s.

## Come funziona

- Il server chiama il nodo su `/node/v1` (`Authorization: Bearer <chiave>`). Il nodo non chiama mai il server, tranne
  la lettura del file da `/api/v1/node-source/<token>` (un token per sessione, valido finché la sessione vive,
  con le richieste Range) e la prova di collegamento (`POST /node/v1/probe`, il nodo chiama `/api/v1/health`).
- La playlist HLS la scrive sempre il server; il nodo usa lo stesso motore del server (`HlsRuntime`: FFmpeg
  riavviato sui salti, sospeso quando è troppo avanti, ripiego GPU → software).
- Codice: `src/node/` (modalità nodo), `src/nodes/` (registro, scelta di esecutori e copie, sessione HLS con failover,
  lettura dalle copie), `src/api/nodes.rs` (API di amministrazione).
- Prova end-to-end: `scripts/nodes-test/run.sh` (server + 2 nodi in container, file sintetici).
