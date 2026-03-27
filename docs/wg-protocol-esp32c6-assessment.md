# WireGuard Protocol as Encryption Layer on ESP32-C6

## Concept

Use WireGuard's cryptographic protocol (Noise_IKpsk2 + ChaCha20-Poly1305 over UDP) as a standalone encryption layer — no VPN tunnel, no kernel module, no TLS/PKI overhead. Based on the idea from [WireGuard Is Two Things](https://www.proxylity.com/articles/wireguard-is-two-things.html).

The article argues WireGuard is two distinct things:

1. **VPN Application** — the `wg` tool, kernel module, and ecosystem for encrypted tunnels.
2. **Cryptographic Protocol** — a stateless encryption spec built on modern cryptographic primitives, independent of VPNs entirely.

The proposal: use the protocol as a drop-in encryption layer for any application that moves data over UDP, without running a VPN at all.

## Target Architecture

```
ESP32-C6 (Embassy, no_std)              macOS Server (Axum)
─────────────────────────              ────────────────────
WiFi → UDP socket                      UDP socket (tokio)
   │                                      │
   ├── Noise_IKpsk2 handshake ──────────►│
   │◄── handshake response ──────────────┤
   │                                      │
   ├── encrypted sensor data (UDP) ─────►│
   │    (ChaCha20-Poly1305)              │ decrypt → process
   │                                      │ → store/forward via Axum
```

- **Edge device:** ESP32-C6 running Rust with Embassy (no_std) via esp-hal
- **Server:** macOS device running Rust with Axum
- **Transport:** UDP with WireGuard protocol encryption
- **Data flow:** Sensor data from microcontroller to centralized server

## Feasibility: Yes, with caveats

The ESP32-C6 (RISC-V, ~160MHz, ~512KB SRAM) is capable enough, and the no_std Rust ecosystem has the required pieces.

### Crypto Primitives (all no_std-compatible)

| Crate                | Role                                        | no_std |
| -------------------- | ------------------------------------------- | ------ |
| `chacha20poly1305`   | Authenticated encryption (AEAD)             | Yes    |
| `x25519-dalek`       | Diffie-Hellman key exchange                 | Yes    |
| `blake2`             | HMAC/KDF (used in WireGuard key derivation) | Yes    |
| `snow`               | Noise protocol framework (Noise_IKpsk2)     | Yes    |

`snow` is the critical piece — it implements the Noise framework in Rust and supports no_std via a `resolver` feature that lets you plug in your own crypto backend. Wire it to use the RustCrypto crates above.

### Networking Stack

- **`esp-hal`** + **`esp-wifi`** — WiFi driver for ESP32-C6
- **`embassy-net`** — async TCP/UDP sockets on top of `esp-wifi`
- UDP is first-class in `embassy-net` via `UdpSocket` with `send_to`/`recv_from`

### WireGuard Protocol Messages

The WireGuard protocol is minimal (4 message types). The embedded side implements:

1. **Handshake Initiator** (message types 1 & 2) — Noise_IKpsk2 via `snow`
2. **Keepalive** (message type 3) — empty encrypted packet to maintain NAT mappings
3. **Transport Data** (message type 4) — ChaCha20-Poly1305 encrypted sensor payloads

### Server Side (macOS / Axum)

On the Axum side, run a `tokio::net::UdpSocket` listener alongside the HTTP server. Use the same `snow` crate (with std) to handle the Noise handshake and decrypt incoming sensor packets. Axum handles the HTTP/API layer; the WireGuard-protocol UDP listener is a separate `tokio::spawn` task.

Decrypted sensor data can be exposed through Axum REST endpoints or WebSocket to dashboards/clients.

## Key Challenges

### 1. `snow` on no_std

It works, but you need to provide a custom `CryptoResolver` to wire in RustCrypto crates. There's no off-the-shelf no_std resolver — expect ~100-200 lines of glue code. This is the main integration effort.

### 2. Memory

A Noise handshake session in `snow` uses ~2-4KB. ChaCha20-Poly1305 is lightweight. The ESP32-C6's 512KB SRAM is plenty, but you'll want a static allocator or arena for session state since you're no_std.

### 3. Randomness

WireGuard needs a CSPRNG for nonces and ephemeral keys. The ESP32-C6 has a hardware RNG (`esp-hal` exposes it). Implement the `rand_core::RngCore` trait for it.

### 4. Timer / Replay Protection

WireGuard uses a TAI64N timestamp for replay protection. On a device with no RTC, either:

- Use a monotonic counter persisted to flash, or
- Accept the first handshake after boot and rely on the nonce counter for replay protection within a session

### 5. No Existing WireGuard Protocol Library for Rust

The .NET `WireGuardClient` from the article doesn't have a Rust equivalent. You're implementing the message framing yourself (fixed headers, Noise payloads, and a counter). Roughly 500-800 lines of protocol code on top of `snow`.

## Noise Protocol Framework Deep Dive

### Core Building Blocks

Noise is a framework for building crypto protocols, designed by Trevor Perrin (co-creator of Signal). It's not a single protocol — it's a set of rules for composing handshake patterns from a small number of DH operations.

Everything in Noise reduces to three primitives:

- **DH** — Curve25519 key agreement
- **AEAD** — ChaCha20-Poly1305 authenticated encryption
- **Hash** — BLAKE2s for key derivation

No negotiation, no cipher suites, no version fields. Both sides know the pattern in advance.

### How Noise_IKpsk2 Works

A pattern is described by a short notation. WireGuard uses **Noise_IKpsk2**, which reads as:

```
IK:
  <- s            (responder's static key is known to initiator beforehand)
  ...
  -> e, es, s, ss (initiator sends ephemeral key, does DH, sends static key, does DH)
  <- e, ee, se    (responder sends ephemeral key, does remaining DHs)
```

In plain terms:

1. **Pre-handshake:** The initiator (ESP32) already knows the server's public key (pre-shared, hardcoded or provisioned). No discovery, no certificates.
2. **Message 1 (initiator → responder):** ESP32 generates an ephemeral key pair, performs two DH operations (ephemeral-static, static-static), and sends its encrypted identity + payload.
3. **Message 2 (responder → initiator):** Server generates its own ephemeral key pair, completes the remaining DH operations (ephemeral-ephemeral, static-ephemeral), sends encrypted confirmation.
4. **Done.** Both sides now share symmetric keys with forward secrecy. Total: **1 round trip**.

The `psk2` suffix means a pre-shared symmetric key is mixed in after message 2, adding a layer of symmetric authentication on top of the asymmetric keys.

### Chaining Key Mechanism

Noise maintains a **chaining key** that evolves with every DH operation. Each new DH result is mixed into this chain via HKDF. This means:

- Every message is encrypted under a progressively stronger key
- Compromise of one ephemeral key doesn't expose past or future sessions (forward secrecy)
- The handshake transcript is implicitly authenticated — any tampering breaks the chain

### After the Handshake

Both sides derive a pair of symmetric keys (one per direction). All subsequent data is encrypted with ChaCha20-Poly1305 using a simple incrementing nonce. No renegotiation, no session tickets, no state machine beyond "increment counter."

## Why Noise Beats DTLS for This Use Case

### DTLS (Datagram TLS)

DTLS is TLS adapted for UDP. It inherits TLS's full machinery:

| Aspect                 | DTLS 1.3                                                                         | Noise_IKpsk2                                             |
| ---------------------- | -------------------------------------------------------------------------------- | -------------------------------------------------------- |
| **Handshake RTTs**     | 1-2 RTT (+ cookie exchange for DoS protection = often 2)                        | 1 RTT, always                                            |
| **Identity**           | X.509 certificates or PSK                                                        | Static Curve25519 keys (32 bytes each)                   |
| **PKI required**       | Yes — CA, cert chains, validation, expiry, revocation                            | No — just exchange public keys out of band               |
| **Cipher negotiation** | Client/server negotiate cipher suite                                             | None — both sides already agree on primitives            |
| **Code complexity**    | Large — retransmission timers, fragment reassembly, epoch tracking, alert protocol | Minimal — two messages, then symmetric encrypt           |
| **State machine**      | ~10 handshake states, retransmit logic for lossy UDP                             | 2 states: handshaking → transport                        |
| **Message overhead**   | Record header + content type + epoch + sequence (13+ bytes)                      | Type (1B) + sender index (4B) + counter (8B) = 13 bytes  |
| **no_std Rust support**| Weak — `rustls` doesn't support DTLS; `mbedtls` bindings exist but are C FFI    | `snow` supports no_std natively with custom resolvers    |

### The Practical Argument for Embedded

1. **No certificates.** On an ESP32 with no filesystem and no clock, managing X.509 certificates is painful. Certificate expiry, chain validation, and revocation checks all become problems you have to work around. With Noise, you flash a 32-byte server public key and a 32-byte device key pair. That's it.

2. **No negotiation means no downgrade attacks.** DTLS cipher suite negotiation is an attack surface. Noise has none — the pattern is fixed at compile time.

3. **Deterministic memory.** A DTLS implementation needs buffers for fragmented handshake messages (DTLS splits large certificate chains across multiple datagrams), retransmission queues, and epoch tracking. Noise's handshake fits in a fixed-size buffer — you know exactly how much RAM it needs at compile time, which is critical for no_std with static allocation.

4. **Simpler loss handling.** DTLS has its own retransmission layer (since UDP is unreliable) with exponential backoff timers for handshake messages. Noise's 1-RTT handshake means: send message 1, wait for message 2, done. If it doesn't arrive, just restart. No retransmission state machine.

5. **Ecosystem fit.** The Rust no_std ecosystem has `snow` (Noise) + RustCrypto crates. There is no mature no_std DTLS implementation in Rust. You'd be wrapping C (`mbedtls` or `wolfssl`) via FFI, which is fragile on RISC-V targets and defeats the purpose of writing Rust.

### The One Thing DTLS Does Better

**Interoperability.** DTLS is a standard that every language and platform supports. If the ESP32 needed to talk to arbitrary third-party servers, DTLS would win. But since both endpoints are controlled (ESP32 + Axum server), interop isn't needed — simplicity and small footprint matter more, which is exactly where Noise excels.

## Comparison to Alternatives

| Approach               | Pros                                          | Cons                                       |
| ---------------------- | --------------------------------------------- | ------------------------------------------ |
| **WireGuard protocol** | No certs/PKI, 1-RTT, UDP, tiny footprint      | Custom framing code, no_std snow glue      |
| **TLS (rustls)**       | Well-tested, standard                         | Certificates, TCP overhead, larger footprint |
| **DTLS**               | UDP-based TLS                                 | Complex, fewer no_std options              |
| **Raw ChaCha20**       | Simplest implementation                       | No key exchange, no forward secrecy        |
| **MQTT+TLS**           | Ecosystem support, pub/sub                    | Heavy, TCP-based, certificate management   |

## Estimated Implementation Effort

- `CryptoResolver` glue for `snow` (no_std): ~100-200 lines
- WireGuard message framing (4 message types): ~500-800 lines
- Embassy UDP + WiFi setup: ~100-200 lines (well-documented in esp-hal examples)
- Server-side UDP listener + `snow` integration: ~300-500 lines
- Axum HTTP layer for sensor data API: standard Axum setup

## References

- [Noise Protocol Framework](https://noiseprotocol.org/noise.html) — the specification for Noise handshake patterns, including IKpsk2.
- [WireGuard: Next Generation Kernel Network Tunnel](https://www.wireguard.com/papers/wireguard.pdf) — the WireGuard whitepaper describing the protocol, message formats, and cryptokey routing.

## Verdict

This is very doable and arguably the right architecture for the use case. The WireGuard protocol approach gives you:

- Mutual authentication via static Curve25519 keys (no certificates)
- 1-RTT handshake (vs TLS 1.3's 1-RTT but with far less complexity)
- UDP transport (no head-of-line blocking, better for lossy WiFi)
- Tiny code footprint relative to a TLS stack

The main work is the `snow` no_std integration and the ~500 lines of WireGuard message framing. Everything else (WiFi, UDP, crypto primitives) is off-the-shelf in the Embassy/esp-hal/RustCrypto ecosystem.
