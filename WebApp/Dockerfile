# syntax=docker/dockerfile:1.7
# Platea web UI: build con Node, esecuzione con nginx su file statici.

FROM node:22-alpine AS build
WORKDIR /app
COPY package.json package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci --no-audit --no-fund
COPY . .
RUN npm run build

FROM nginx:1-alpine
# Correzioni di sicurezza di Alpine uscite dopo l'immagine base (controllate da Trivy in CI).
# nginx gira come utente "nginx", non root: pid in /tmp e cartelle scrivibili per lui. La porta 80 resta
# utilizzabile perché Docker consente le porte basse ai processi non root dentro il container.
RUN apk upgrade --no-cache \
 && sed -i -e '/^user /d' -e 's#^pid .*#pid /tmp/nginx.pid;#' /etc/nginx/nginx.conf \
 && chown -R nginx:nginx /var/cache/nginx /etc/nginx/conf.d
USER nginx
# Il template viene completato da nginx all'avvio con PLATEA_SERVER_URL.
COPY nginx.conf.template /etc/nginx/templates/default.conf.template
COPY --from=build /app/dist /usr/share/nginx/html
ENV PLATEA_SERVER_URL=http://server:8096
EXPOSE 80
HEALTHCHECK --interval=30s --timeout=3s CMD wget -qO- http://127.0.0.1/healthz >/dev/null || exit 1
