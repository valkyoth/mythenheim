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
- Cookie transport will be `HttpOnly`, `Secure`, and strict same-site by default
  once HTTP auth routes are added.
- Tokens must be revocable server-side through the `session.revoked_at` field.

## Current Implementation

The current slice implements the primitives:

- `auth::password::hash_password`
- `auth::password::verify_password`
- `auth::session::NewSessionToken::generate`
- `auth::session::hash_session_token`
- `auth::session::verify_session_token_hash`

HTTP registration/login endpoints are the next step in the `0.12.0` milestone.
