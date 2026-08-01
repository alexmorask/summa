FROM postgres:17-alpine

RUN apk add --no-cache ca-certificates

COPY db/migrations /migrations
COPY db/migrate.sh /migrate.sh

ENTRYPOINT ["/migrate.sh"]
