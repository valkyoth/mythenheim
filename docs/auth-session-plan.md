# Authentication And Session Plan

Mythenheim's auth design uses password login only as one supported credential
path. Passkeys and SSO come later, but password handling and session handling
must be safe from the beginning.

## Passwords

- Password hashes use Argon2id through the RustCrypto `argon2` crate.
- The current crate defaults are Argon2id `m=19456`, `t=2`, `p=1`, matching the
  current OWASP Password Storage Cheat Sheet baseline.
- Password input is bounded before hashing to avoid unbounded login CPU/memory
  work.
- Passwords shorter than 12 bytes are rejected.
- Passwords longer than 1024 bytes are rejected.
- NUL bytes are rejected to avoid ambiguous handling across future import or
  integration boundaries.
- Stored password hashes are PHC strings. The verifier uses parameters embedded
  in the stored hash.

Reference: [OWASP Password Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html).

## Sessions

- Primary sessions use opaque random tokens, not primary stateless JWTs.
- A token contains 32 random bytes encoded as lowercase hexadecimal.
- Only a SHA-256 hash of the token is intended to be stored in the database.
- Cookie transport is `HttpOnly`, strict same-site, and `Secure` by default.
- Tokens must be revocable server-side through the `session.revoked_at` field.

## Current Implementation

The current slice implements the primitives and a preview HTTP auth surface:

- `auth::password::hash_password`
- `auth::password::verify_password`
- `auth::session::NewSessionToken::generate`
- `auth::session::hash_session_token`
- `auth::session::verify_session_token_hash`
- `auth::service::AuthService::register`
- `auth::service::AuthService::login`
- `auth::service::AuthService::authenticate`
- `auth::service::AuthService::logout`

The preview service currently uses an in-memory store so route behavior can be
tested before SurrealDB persistence is wired into the service layer. Session
tokens are still only compared and stored in hashed form inside that store.

## HTTP Preview

The first `0.12.0` HTTP slice exposes:

- `POST /api/v1/auth/register`
- `POST /api/v1/auth/login`
- `GET /api/v1/auth/me`
- `POST /api/v1/auth/logout`

The route tests verify registration, login, secure cookie issuance, authenticated
identity lookup, logout revocation, duplicate username rejection, and oversized
request rejection.
