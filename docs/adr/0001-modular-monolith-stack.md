# ADR-0001: Modular monolith — Rust/Axum, Vue 3, PostgreSQL

**Status:** Accepted · **Date:** 2026-11-05 · **Deciders:** Owner

## Context
One shop, low traffic, one maintainer, and it's built with Claude.
Correctness of stock and money matters far more than scale.

## Decision
One Rust (Axum + Tokio + SQLx) service split into modules by business
area, a Vue 3 + TypeScript PWA, and PostgreSQL. No Redis, queues,
microservices or Kubernetes.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| **Rust/Axum + SQLx** (chosen) | Strong types for money, states and permissions; compile-time checked SQL; one small binary; low memory | Slower to write; smaller ecosystem for admin tooling |
| Node/TypeScript (Fastify + Kysely) | One language front and back; faster to iterate | Weaker guarantees around decimals and exhaustive state handling |
| Django/Rails | Admin screens and auth built in | Second language; dynamic typing around money rules |
| Microservices | Independent scaling | No benefit at this size; much more to operate |

## Consequences
- Easier: making invalid states impossible (enums for statuses),
  catching SQL mistakes at build time, cheap hosting.
- Harder: compile times; SQLx's offline query data (`sqlx prepare`)
  must be kept up to date in CI.
- Revisit if a second team or a very different workload appears.
