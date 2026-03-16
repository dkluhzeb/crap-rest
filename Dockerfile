FROM alpine:3.20
RUN apk add --no-cache ca-certificates
ARG BINARY=crap-rest-linux-x86_64
COPY ${BINARY} /usr/local/bin/crap-rest
RUN chmod +x /usr/local/bin/crap-rest
EXPOSE 8080
ENTRYPOINT ["crap-rest"]
