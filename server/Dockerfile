# Self-hosted sync server: one Node process, one data directory, no dependencies.
#
#   docker build -t alloy-sync /path/to/server
#   docker run -d --name alloy-sync -p 8080:8080 \
#     -e SYNC_TOKENS=token1,token2:67108864 \
#     -v alloy-sync-data:/data alloy-sync
#
# Terminate TLS in front of it (see README) — this container speaks plain HTTP.

FROM node:22-alpine

WORKDIR /app
COPY package.json ./
COPY src ./src

RUN mkdir -p /data && chown node:node /data
USER node

ENV SYNC_DATA_DIR=/data \
    PORT=8080 \
    HOST=0.0.0.0

VOLUME ["/data"]
EXPOSE 8080

CMD ["node", "src/main.js"]
