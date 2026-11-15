# Sicurezza

## Segnalare una vulnerabilità

Non aprire una issue pubblica. Usa la segnalazione privata di GitHub:
**Security › Report a vulnerability** in questo repository. Indica la versione, come riprodurre il problema e il
possibile impatto. Rispondiamo appena possibile e ti teniamo aggiornato fino alla correzione; se vuoi, sarai
citato nelle note della versione.

## Versioni supportate

Le correzioni di sicurezza escono sull'ultima versione pubblicata. Aggiorna con `docker compose pull && docker compose up -d`
(o `git pull && docker compose up -d --build`): le migrazioni del database partono da sole dopo una copia di sicurezza.

## Consigli per l'installazione

- Esponi Platea su Internet solo tramite HTTPS (reverse proxy o tunnel), mai la porta 8096 in chiaro.
- Tieni i container aggiornati: la CI controlla le immagini con Trivy.
- Le cartelle dei media vanno montate in sola lettura (`:ro`), come negli esempi del README.
- Per gli utenti che non devono uscire dalla rete di casa togli il permesso "accesso da remoto".
