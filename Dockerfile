FROM oven/bun:1.3 AS builder

WORKDIR /root

COPY package.json bun.lock ./
RUN bun install

COPY . .
RUN bun build index.ts --outdir ./dist --target bun --format esm
