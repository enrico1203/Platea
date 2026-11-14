# syntax=docker/dockerfile:1.7
# Platea web UI: build con Node, esecuzione con nginx su file statici.

FROM node:22-alpine AS build
WORKDIR /app
COPY package.json package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci --no-audit --no-fund
COPY . .
RUN npm run build

FROM nginx:1-alpine
# Il template viene completato da nginx all'avvio con PLATEA_SERVER_URL.
COPY nginx.conf.template /etc/nginx/templates/default.conf.template
COPY --from=build /app/dist /usr/share/nginx/html
ENV PLATEA_SERVER_URL=http://server:8096
EXPOSE 80
HEALTHCHECK --interval=30s --timeout=3s CMD wget -qO- http://127.0.0.1/healthz >/dev/null || exit 1
