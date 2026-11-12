# Authentication and key custody for native ikigai apps

An assessment (ledger #3534, split from the security audit #3525). It is a written report, with no
implementation. It decides how stage B of the GPUI reader (#3524, the "collaborator mode") holds a
collaborator's identity, recommends a path for a mobile app, and lists the
[ikigai-secret](https://github.com/ikigai-rs/ikigai-secret) backends to add first.

It is written against these versions, all read on 2026-11-12: ikigai-secret `main` at
[`792de1e`](https://github.com/ikigai-rs/ikigai-secret/tree/792de1e11326db492fd9ff68a2b58b6d3230aa6a)
(0.4.0), ikigai-layers `main` ([README](https://github.com/ikigai-rs/ikigai-layers#readme)),
ikigai-cli `main` ([README](https://github.com/ikigai-rs/ikigai-cli#readme)), ikigai-sign
`main` ([README](https://github.com/ikigai-rs/ikigai-sign#readme)), ikigai-cms-web `main`
([README](https://github.com/ikigai-rs/ikigai-cms-web#readme)), and this repository's
`apps/reader`.

## Summary

- **The reader (desktop):** a **device key** in the best custody the platform has (the Secure
  Enclave on a Mac, a TPM 2.0 key on Windows and Linux), held by ikigai-secret and used by
  **sign-through**, never read out. It is the reader's mTLS client certificate key, which layers
  already understands. A collaborator **pairs** the reader by approving its certificate's
  fingerprint from a browser where they are already signed in with a passkey: the RFC 8628
  user-code shape, except that what the approval issues is an `onb:Entitlement` for the device's
  own key, so no bearer token ever reaches the device.
- **Mobile:** an in-app **platform passkey** (ASAuthorization on iOS, Credential Manager on
  Android) authenticates the person and, in the same ceremony, enrolls a **Secure Enclave or
  StrongBox P-256 device key**. Requests then go to the HTTPS front with short-lived access
  tokens that are **DPoP-bound** (RFC 9449) to that key, instead of today's long-lived bearer
  token.
- **ikigai-secret first:** (1) sign-through on the resource face under its own capability, with
  a hardware `generate`; (2) Windows: the presence gate through Windows Hello, a TPM key through
  CNG, and DPAPI for byte secrets; (3) Linux: a TPM 2.0 key, and Secret Service for byte
  secrets; (4) a `custody` descriptor so an app can tell a hardware key from a file. See
  [Items for the hub](#items-for-the-hub).

## What exists today

| mechanism | where | what it authenticates | long-lived material, and where it lives |
| --- | --- | --- | --- |
| mTLS client certificates, mutually pinned, over QUIC | [ikigai-cli](https://github.com/ikigai-rs/ikigai-cli#readme) `serve quic://`, `clients.json` | a device (certificate fingerprint) mapped to a participant's grant | the client key, a file under the config directory (`ikigai cert generate`) |
| Open join from a fresh certificate | [ikigai-layers](https://github.com/ikigai-rs/ikigai-layers#public-layers-and-joining), the join lobby | a stranger's certificate, granted `read` and `feed` on public feeds | the client key (the reader keeps it in `~/.config/ikigai-reader/client.key`, `0600`) |
| Onboarding by signed entitlement | ikigai-layers, lobby plus grants layer | a certificate approved by `onboard.controller`'s ikigai-sign signature | the controller's signing key; the node's client key |
| HTTPS front, bearer tokens | ikigai-cli `--layers-https`, `ikigai layers-token mint` | an enrolled participant, one token each | the token, held by the client however it likes; the door keeps only its SHA-256 hash |
| Passkeys on the edge's HTTP face | [ikigai-cms-web](https://github.com/ikigai-rs/ikigai-cms-web#readme) (`webauthn-rs`), `ikigai-passkey` on the edge's decision links | a person, in a browser | the passkey, in the person's authenticator; the relying party's credential store in ikigai-secret (`cms-passkeys`) |
| Signature graphs | [ikigai-sign](https://github.com/ikigai-rs/ikigai-sign#readme) | a statement, by Ed25519 or ES256 | the signing key, `key=urn:secret:<name>` |
| Capabilities | ikigai-core `urn:cap:*` scopes, declared = enforced | what a caller may do, narrowed per grant | grants minted by the door from `clients.json`/`grants.json` per connection |
| Keystores | [ikigai-secret](https://github.com/ikigai-rs/ikigai-secret#readme) | (custody, not authentication) | the macOS Keychain, the Secure Enclave, or a file store |

What ikigai-secret actually does today, read from its source:

- `default_backend()` is the **Keychain** on macOS and the **file store everywhere else**
  ([`src/lib.rs`](https://github.com/ikigai-rs/ikigai-secret/blob/792de1e11326db492fd9ff68a2b58b6d3230aa6a/src/lib.rs#L259-L270)).
  The file store sets `0700`/`0600` modes on Unix only; on Windows it has no ACL hardening
  (an NTFS follow-up is already on ledger #3504).
- The **presence gate** is `require_biometric`, which calls
  [robius-authentication](https://crates.io/crates/robius-authentication) with a strong-biometric
  policy, but only under `cfg(target_os = "macos")`; everywhere else it returns `Ok(())`. The
  crate it calls also supports Windows (Windows Hello) and Android. The gate is
  **advisory and in-process**: it is a prompt the endpoint asks before it reads, not an access
  control on the Keychain item. Another process with access to the item reads it with no prompt.
- `urn:secret:generate type=ed25519` mints the key **in software** (`ed25519-dalek`) and stores
  the PKCS#8 PEM as bytes. On a Mac that is a Keychain generic password; elsewhere it is a file.
  The private half is **extractable**: anyone holding `urn:cap:secret:read:<name>` gets the bytes.
- The **Secure Enclave backend** (`SecureEnclaveBackend`) exists, but only as a Rust API: it makes
  a non-extractable **P-256** key and signs through it (`Backend::sign`,
  `Backend::generate_signing_key`). It is not `default_backend()`, `generate` never uses it, and
  **no resource exposes sign-through**. Its keys are deliberately made with no biometric
  requirement, so a daemon can sign unattended.

The reader (`apps/reader/src/lib.rs`, `Identity::load_or_create`) makes its own Ed25519 key with
`rcgen` and writes it to `client.key` with `0600`. It does not use ikigai-secret at all. So the
gap this item names is real: **on Linux and Windows a GPUI app has no hardware-backed or
OS-protected custody today, and on a Mac the reader does not use what exists.**

## The options

Each option says what it authenticates, where its long-lived material lives (**custody**), and
how it maps onto ikigai's capabilities.

### 1. Platform passkey APIs from a native app

- **Apple:** [`ASAuthorizationPlatformPublicKeyCredentialProvider`](https://developer.apple.com/documentation/authenticationservices/asauthorizationplatformpublickeycredentialprovider)
  (iOS 15+, macOS 12+) creates and asserts passkeys from a native app. The app must hold the
  `webcredentials:` [associated domain](https://developer.apple.com/documentation/xcode/supporting-associated-domains)
  for the relying party ID, and the RP must serve `apple-app-site-association`. See
  [Supporting passkeys](https://developer.apple.com/documentation/authenticationservices/supporting-passkeys).
- **Windows:** `webauthn.dll` ([microsoft/webauthn](https://github.com/microsoft/webauthn)) gives a
  native app the same WebAuthn ceremonies, with Windows Hello as the platform authenticator and
  roaming keys and phones (hybrid) behind it. Since Windows 10 1903, a non-administrator process
  cannot talk to a FIDO HID device directly; it must go through this API.
- **Android:** [Credential Manager](https://developer.android.com/identity/sign-in/credential-manager),
  with [Digital Asset Links](https://developer.android.com/training/app-links/verify-android-applinks)
  tying the app to the RP. The assertion's origin is `android:apk-key-hash:…`, which the relying
  party must allow explicitly.
- **Linux:** **no** platform authenticator or system passkey API ships today. Browsers implement
  WebAuthn themselves, with USB security keys and their own hybrid support; a native app gets
  only raw CTAP2 (option 5).

**Custody:** the passkey lives in the platform's authenticator (iCloud Keychain, Google Password
Manager, Windows Hello, a security key). ikigai never holds it. **What it gives:** the person, at
sign-in, not the device afterwards: the app still needs session material after the ceremony,
and that is what custody is about. **Capabilities:** the edge already raises a connection's
ceiling on a verified passkey (cms-web). Issuance is the RP's; revocation is deleting the
credential at the RP; a lost device is covered when the passkey is synced and the account
removes it. **Offline:** none. **Desktop GPUI:** feasible on macOS and Windows, absent on Linux,
and it needs an app identity (signed bundle and associated domain), which a `cargo run` spike
does not have.

### 2. OAuth 2.0 Device Authorization Grant (RFC 8628) to pair through a browser session

[RFC 8628](https://www.rfc-editor.org/rfc/rfc8628): the device shows a short user code; the
person opens a URL in any browser, signs in there (here, with a passkey on the edge), enters the
code and approves; the device polls and receives its credential.

**Custody:** in RFC 8628 the device receives an access and a refresh token, so the long-lived
material is a **refresh token**, a bearer secret. ikigai-secret can hold it as a named secret
(`reader.refresh`) under `urn:cap:secret:read:reader.refresh`, but a bearer secret read into
process memory is exactly what stealing malware takes. **The ikigai fit is better than the RFC's:**
the device already has a key and a certificate, so what the approval issues can be an
`onb:Entitlement` for the device's **certificate fingerprint**, which layers' onboarding already
applies, verifies and revokes. Nothing bearer crosses. **Risks:** remote phishing of the user code
([RFC 8628 §5.4](https://www.rfc-editor.org/rfc/rfc8628#section-5.4)): an attacker starts a pairing
on their own device and lures the victim into approving it. The approval page must show the
device's name and a fingerprint the app also shows, the code must expire in minutes, and a new
pairing should start with the least grant. **Capabilities:** issuance is the entitlement;
attenuation is the entitlement granting a subset of the participant's verbs; revocation is a
superseding entitlement with no `onb:layerGrant`; a lost device is a revocation of one
fingerprint. **Offline:** none for pairing; the entitlement itself verifies offline (it is a
signature graph). **Linux:** works, because only a browser is needed.

### 3. mTLS device certificates with keys in the OS keystore or Secure Enclave

What layers already does, with the key moved out of a file. A self-signed client certificate's
key is generated non-extractable in the
[Secure Enclave](https://support.apple.com/guide/security/secure-enclave-sec59b0b31ff/web)
([protecting keys with it](https://developer.apple.com/documentation/security/protecting-keys-with-the-secure-enclave)),
a [TPM 2.0](https://learn.microsoft.com/en-us/windows/security/hardware-security/tpm/tpm-fundamentals)
(through CNG's Platform Crypto Provider on Windows, through
[tpm2-tss](https://github.com/tpm2-software/tpm2-tss) on Linux), or Android's
[StrongBox](https://developer.android.com/privacy-and-security/keystore). The TLS stack signs the
handshake's `CertificateVerify` through the key (rustls takes a custom `SigningKey`).

**Custody:** the device key, non-extractable. Malware running as the user can **use** it while it
runs, but cannot **copy** it, so revoking the device ends the attack. **The algorithm constraint:**
the Secure Enclave, TPM 2.0 (in practice) and StrongBox all do **P-256**, not Ed25519. TLS 1.3's
`ecdsa_secp256r1_sha256` is a P-256 signature, and ikigai-sign has verified ES256 since 0.2.0,
so P-256 is no obstacle. One detail: TLS wants a DER signature, while ikigai-secret's `Signed`
returns fixed `r‖s`, so the TLS adapter re-encodes it. **Capabilities:** this is already the
ikigai model: the door maps the fingerprint to a grant per connection, and `clients.json`,
`enrollment.toml` and entitlements are the issuance and revocation. A lost device is one
fingerprint revoked; with a non-extractable key nothing else needs rotating. **Offline:** n/a,
since the connection is the point. **Mobile caveat:** `URLSession` presents a client certificate
only as a `SecIdentity`, and many HTTP stacks make client certificates awkward, which is why the
mobile recommendation binds tokens instead of the TLS session.

### 4. DPoP-bound (RFC 9449) or other proof-of-possession tokens instead of bearer tokens

[RFC 9449](https://www.rfc-editor.org/rfc/rfc9449): each request carries a JWS (the DPoP proof)
signed by the client's key over the method, URL, time and a nonce, and the access token names that
key's thumbprint, so a stolen token is useless without the key. The alternative binding,
[RFC 8705](https://www.rfc-editor.org/rfc/rfc8705), binds a token to the mTLS certificate instead
(option 3 again).

**Custody:** the DPoP key (P-256 `ES256` in hardware; `EdDSA` per
[RFC 8037](https://www.rfc-editor.org/rfc/rfc8037) in software) plus a **short-lived** token,
which can then sit in memory only. Today `layers-token` mints a long-lived bearer token whose loss
is a full impersonation until someone revokes it. **Capabilities:** the HTTPS front would mint
the token with the participant's grant, as it does now, and additionally check the proof against
the bound key; revocation stays `layers-token revoke` plus removing the key's binding.
**Cost:** the HTTPS front has no DPoP today (new work in ikigai-cli), and replay protection
needs a nonce or a `jti` cache. **Offline:** none.

### 5. FIDO2/CTAP2 security keys used directly by the app

[CTAP2](https://fidoalliance.org/specifications/) through
[libfido2](https://github.com/Yubico/libfido2) or a Rust CTAP crate. A security key can assert
like a passkey (the RP ID is whatever the app claims, since no browser enforces the origin) and,
with the `hmac-secret` extension, derive a stable secret that unlocks local material.

**Custody:** the credential lives on the key; `hmac-secret` lets a file-store secret be wrapped
under a key only the token can unwrap, which is a real improvement on Linux. **Limits:** on
Windows a non-admin app must use `webauthn.dll` instead (option 1); everyone needs a physical key,
which suits an operator and not a broad set of collaborators; and an app asserting for any RP ID
is phishing-resistant only if the server pins the RP ID it expects. **Capabilities:** a strong
presence gate (`PresenceGate` as "touch the key") and a second factor for issuance; it does not
replace the device identity.

### 6. OS biometrics unlocking a local key

Touch ID or Face ID (`LocalAuthentication`), Windows Hello
([`UserConsentVerifier`](https://learn.microsoft.com/en-us/uwp/api/windows.security.credentials.ui.userconsentverifier),
or a key from
[`KeyCredentialManager`](https://learn.microsoft.com/en-us/uwp/api/windows.security.credentials.keycredentialmanager),
which is TPM-backed and asks Hello on each use), Android `BiometricPrompt`. Linux has `fprintd`
but no standard user-presence API an app can rely on.

**What it is:** **presence**, not authentication to a server. It is only as strong as where it is
enforced: ikigai-secret's gate today is a prompt in the calling process. **Bound into the key's
access control** (a Secure Enclave key with `kSecAccessControlUserPresence` or
`.biometryCurrentSet`, an Android key with `setUserAuthenticationRequired`, a Hello key) the
hardware refuses to sign without the gesture, and malware in the process cannot skip it. That is
right for an interactive app's key, and wrong for a daemon's (which is why the existing Enclave
backend has none). **Capabilities:** it maps onto `PresenceGate` and, better, onto a backend's key
policy; it says nothing about issuance or revocation.

### Custody at a glance

| option | long-lived material on the device | can ikigai-secret hold it today? | after the items below |
| --- | --- | --- | --- |
| 1 platform passkey | none (the authenticator holds it); a session after | n/a | n/a |
| 2 device grant, as in the RFC | a refresh token (bearer) | yes, as a named secret; a file off macOS | DPAPI / Secret Service; still a bearer secret |
| 2 device grant, ikigai shape | the device key (an entitlement is public) | in software only | non-extractable, sign-through |
| 3 mTLS device certificate | the device key | software (Ed25519 via `generate`); the Enclave from Rust only | Enclave, TPM on Windows and Linux, via a resource |
| 4 DPoP | the device key; tokens in memory | as 3 | as 3 |
| 5 CTAP2 security key | none (on the key), or a wrapped secret | the wrapped blob, as a file | the same, with the key as presence |
| 6 OS biometrics | none (it guards another key) | a macOS prompt; elsewhere a no-op | Windows Hello; bound into key policy |

## The gap on Linux and Windows

Today a GPUI app there stores secrets in the file store, protected by file modes (on Unix) and
nothing else, with no presence. Backends that would close it:

| backend | protects against | does **not** protect against |
| --- | --- | --- |
| **Linux [Secret Service](https://specifications.freedesktop.org/secret-service-spec/latest/)** (GNOME Keyring, KWallet, KeePassXC, via D-Bus or libsecret) | the disk read while the collection is locked (encrypted under the login password); a backup or another user | **any process in the user's session** once the collection is unlocked (which is from login on): there is no per-application access control. Headless and SSH sessions often have no service. |
| **Linux TPM 2.0 key** ([tpm2-tss](https://github.com/tpm2-software/tpm2-tss), [tpm2-pkcs11](https://github.com/tpm2-software/tpm2-pkcs11)) | copying the key off the machine, even by root or malware; a stolen disk | malware **using** the key while it runs as the user; the user needs access to `/dev/tpmrm0` (the `tss` group on many distributions); P-256 and RSA only, so no Ed25519; byte secrets only by sealing |
| **Windows Credential Manager / [DPAPI](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)** | another user; offline disk theft (keys derive from the user's logon secret) | **any process running as the same user**, which can call `CryptUnprotectData` too |
| **Windows TPM key through CNG** (Microsoft Platform Crypto Provider) | copying the key off the machine | use by malware as the user, unless the key requires a Hello gesture |
| **Windows Hello as the presence gate** (`UserConsentVerifier`; robius-authentication already supports it) | an unattended use of a gated operation | malware in the same process (an in-process prompt); for that, bind the gesture to the key (`KeyCredentialManager`) |

The pattern is the same as on a Mac: **byte secrets** (a token, a passkey store, an `age`
identity) go to the OS store, which keeps them from other users and from a stolen disk; **keys
that only sign** go into hardware, which keeps them from being copied at all.

## Identity keys: can `generate`'s Ed25519 key be the device identity?

**Technically, yes.** One Ed25519 key can be a self-signed client certificate for layers' mTLS
(the reader already does exactly that with `rcgen`; rustls accepts Ed25519 in TLS 1.3), a DPoP key
(`EdDSA`, RFC 8037), and an ikigai-sign key. **But as `generate` makes it today, it is a software
key whose bytes any holder of `urn:cap:secret:read:<name>` can read**, so it is no better custody
than the reader's `client.key`, only better gated. And the hardware that would protect it does not
do Ed25519.

The recommendation:

1. **The device identity is a non-extractable P-256 key**, named `<device>.device` (for example
   `brian-mbp.device`), generated in the best backend there is and used only by sign-through. It
   is the mTLS certificate's key and the DPoP key: both **authenticate this device**, so they share
   a lifetime and a revocation. Where no hardware exists, the same name holds a software key, and
   the app says so (item 4 below).
2. **Signing keys for signature graphs stay separate names** (`brian.sign`). A signature graph is
   meant to verify for years; a device key is revoked the day the laptop is lost. If they were one
   key, losing the device would cast doubt on every graph the person ever signed. The device key
   may still sign statements **about itself** (a pairing request, an attestation that this device
   holds this certificate).
3. Cross-protocol reuse of the one device key between TLS and DPoP is safe: TLS 1.3 signs a
   `CertificateVerify` prefixed by 64 spaces and a context string, and a DPoP proof signs a
   base64url JWS signing input, so neither can be replayed as the other.

So: **one device, one device key, one custody**, and a person's long-lived signing identity kept
apart from it.

## Recommendation: the reader's collaborator mode (#3524 stage B)

**Device-key mTLS, paired through a passkey session.**

1. On first run the reader asks ikigai-secret for a device key, `urn:secret:generate
   into=<device>.device type=p256` (item 1), which lands in the Secure Enclave on a Mac and a TPM
   on Windows and Linux (items 2 and 3), the file store only where neither exists. It holds
   `urn:cap:secret:sign:<device>.device` and **no** read scope for it. It makes its self-signed
   certificate from the public half, as `Identity::load_or_create` does today.
2. Public feeds keep working as now: open join from that certificate.
3. **Pairing.** "Sign in as a collaborator" posts an `onb:EnrollmentRequest` to the lobby (which
   already proves the poster holds the certificate) and shows a short code plus a few words derived
   from the fingerprint. The person opens the edge in a browser, signs in with their passkey, enters
   the code, checks that the words match, and approves. The approval yields an `onb:Entitlement`
   naming that fingerprint, which the host applies like any other.
4. Revocation, a lost device, a new device: a superseding entitlement with no grant revokes one
   fingerprint, and nothing else needs rotating because the key never left the hardware.

**Why this one:** it reuses what layers already verifies, applies and revokes (certificate
fingerprints, signed entitlements, per-connection grants); nothing bearer is ever stored; it works
on Linux, which has no platform passkey API, because the passkey is used in a browser; and the
same key serves the QUIC door and, through DPoP, the HTTPS front.

**Risks and open design points:**

- **Who signs the entitlement.** Today only `onboard.controller`'s key does. Letting the edge sign
  after a passkey approval gives the edge a controller's authority. Narrower: a **pairing key**
  whose entitlements the host accepts only for a participant `{person}-{device}` that
  `prov:actedOnBehalfOf` `{person}` and grants **at most** that person's grants (attenuation, not
  a new authority). That is a layers change and needs its own item.
- **One certificate per participant.** Layers holds "a participant is one certificate's", so a
  person with a laptop and a phone needs a participant per device (the `bobby-claude` pattern
  already in the participants graph). That is also what makes per-device revocation clean.
- **Code phishing** (RFC 8628 §5.4): mitigated by fingerprint words on both screens, a short expiry,
  rate limits the lobby already has, and an approval page that names the device and when it asked.
- **No hardware.** Linux without an accessible TPM falls back to a software key in the file store.
  The reader must say so in its settings, and a host may refuse to entitle such a device to write.
- **Presence.** For an interactive reader, the device key should require presence for **pairing and
  for writes** only, not for every TLS handshake, or every reconnect prompts.

## Recommendation: mobile

**An in-app platform passkey that enrolls a hardware device key; DPoP-bound short-lived tokens to
the HTTPS front.**

1. The app performs a passkey assertion with ASAuthorization (iOS) or Credential Manager (Android)
   against the edge's RP ID. Before it, it generates a P-256 key in the Secure Enclave or StrongBox,
   with user presence in the key's access control, and puts the hash of its public key into the
   ceremony's challenge, so the server learns "this person, with this passkey, enrolls this device
   key" in one step.
2. The edge records the device key as a participant `{person}-{device}` (as above), and the HTTPS
   front mints **short-lived** access tokens bound to the key's thumbprint (RFC 9449). Every request
   carries a DPoP proof; the token lives in memory only.
3. A lost phone is one device key revoked; the person's passkey (synced) still works on their next
   phone, which enrolls a new device key.

**Why this one:** on a phone the browser and the app are the same device, so pairing through a
browser adds steps without adding safety; passkeys are native there; mobile HTTP stacks handle a
header far better than a client certificate; and nothing long-lived and bearer is stored.

**Risks:**

- The app needs an associated domain (iOS) and Digital Asset Links (Android) for the edge's RP ID,
  and the edge must serve both files; changing the RP ID invalidates every passkey (cms-web already
  warns about this for browsers).
- The relying party must accept the Android origin form `android:apk-key-hash:…`.
- **New work:** DPoP verification and short-lived token minting on the HTTPS front (ikigai-cli),
  and the edge-to-door issuance bridge (the same "who signs" question as the reader).
- iOS Secure Enclave and Android StrongBox exist on most current devices but not all; a device
  without them gets a Keystore or Keychain software key and should be told so.
- ikigai-secret has no iOS or Android backend yet (item 5).

## Items for the hub

The ikigai-secret backends and changes to add first, in order, each with its reason. ikigai-secret
is public, so workers can take them.

1. **Sign-through as a resource, and hardware `generate`.** A `urn:secret:sign` operation (or a sign
   verb on `urn:secret:{name}`) under its own scope `urn:cap:secret:sign:<name>`, which calls
   `Backend::sign`; `generate type=p256` that calls `Backend::generate_signing_key` on a backend that
   has hardware and refuses (or says plainly it fell back) where it does not; ikigai-sign's
   `key=urn:secret:<name>` signing through it. *Reason:* without it, a caller who can sign with a key
   can also read it, the Secure Enclave backend is unreachable through the kernel, and none of the
   recommendations above can hold a key non-extractably.
2. **Windows.** (a) Turn the presence gate on through Windows Hello: robius-authentication, which
   the gate already calls, supports Windows, so this is mostly a `cfg` change and a test. (b) A CNG
   backend: a TPM-backed P-256 key through the Platform Crypto Provider for sign-through, and DPAPI
   (or Credential Manager) for byte secrets. *Reason:* the Windows GPUI app (#3526) has no
   OS-protected custody and no presence today, and the file store has no NTFS ACL hardening (#3504).
3. **Linux.** (a) A TPM 2.0 sign-through backend (the `tss-esapi` crate over tpm2-tss) for P-256
   device keys. (b) A Secret Service backend for byte secrets, documented honestly: it keeps secrets
   from other users and a stolen disk, not from other processes in the session. (c) An optional
   presence gate on a FIDO2 key touch, since Linux has no standard biometric presence API.
   *Reason:* the same gap as Windows; the TPM is what makes a lost or infected Linux machine
   revocable by one fingerprint.
4. **A custody descriptor.** `Backend::custody()` (`hardware`, `os-protected`, `file`), and the
   same on `urn:secret:{name}`'s description or a `urn:secret:custody` resource, so an app can show
   "this key is a file" and a host can decide what to entitle. *Reason:* `default_backend()` silently
   returns a file store off macOS; every recommendation above needs to know what it got.
5. **Presence bound into the key, and mobile backends.** An Enclave (and later TPM, Android) key
   option whose access control requires user presence, for interactive keys (the existing Enclave
   keys stay unattended, for daemons); then iOS (the same Security.framework code under
   `target_os = "ios"`) and Android Keystore/StrongBox backends when the mobile app starts.
   *Reason:* an in-process prompt is skipped by malware in the process; a key policy is not.

Outside ikigai-secret, the recommendations also need (for the hub to file where it sees fit):
a pairing key in ikigai-layers' onboarding that may only entitle `{person}-{device}` participants
with at most that person's grants; DPoP on ikigai-cli's HTTPS front; and the reader's
`Identity` moved onto ikigai-secret once item 1 lands.

## Sources

- RFC 8628, [OAuth 2.0 Device Authorization Grant](https://www.rfc-editor.org/rfc/rfc8628)
- RFC 9449, [OAuth 2.0 Demonstrating Proof of Possession (DPoP)](https://www.rfc-editor.org/rfc/rfc9449)
- RFC 8705, [OAuth 2.0 Mutual-TLS Client Authentication and Certificate-Bound Access Tokens](https://www.rfc-editor.org/rfc/rfc8705)
- RFC 8037, [CFRG Elliptic Curves for JOSE (EdDSA)](https://www.rfc-editor.org/rfc/rfc8037)
- RFC 8252, [OAuth 2.0 for Native Apps](https://www.rfc-editor.org/rfc/rfc8252)
- W3C, [Web Authentication Level 3](https://www.w3.org/TR/webauthn-3/); FIDO Alliance, [specifications (CTAP)](https://fidoalliance.org/specifications/)
- Apple, [Supporting passkeys](https://developer.apple.com/documentation/authenticationservices/supporting-passkeys), [ASAuthorizationPlatformPublicKeyCredentialProvider](https://developer.apple.com/documentation/authenticationservices/asauthorizationplatformpublickeycredentialprovider), [Supporting associated domains](https://developer.apple.com/documentation/xcode/supporting-associated-domains), [Secure Enclave](https://support.apple.com/guide/security/secure-enclave-sec59b0b31ff/web), [Protecting keys with the Secure Enclave](https://developer.apple.com/documentation/security/protecting-keys-with-the-secure-enclave)
- Microsoft, [WebAuthn APIs for Windows (microsoft/webauthn)](https://github.com/microsoft/webauthn), [CryptProtectData (DPAPI)](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata), [TPM fundamentals](https://learn.microsoft.com/en-us/windows/security/hardware-security/tpm/tpm-fundamentals), [UserConsentVerifier](https://learn.microsoft.com/en-us/uwp/api/windows.security.credentials.ui.userconsentverifier), [KeyCredentialManager](https://learn.microsoft.com/en-us/uwp/api/windows.security.credentials.keycredentialmanager)
- Android, [Credential Manager](https://developer.android.com/identity/sign-in/credential-manager), [Android Keystore system](https://developer.android.com/privacy-and-security/keystore), [Verify Android App Links (Digital Asset Links)](https://developer.android.com/training/app-links/verify-android-applinks)
- freedesktop.org, [Secret Service API](https://specifications.freedesktop.org/secret-service-spec/latest/); [tpm2-tss](https://github.com/tpm2-software/tpm2-tss); [tpm2-pkcs11](https://github.com/tpm2-software/tpm2-pkcs11); Yubico, [libfido2](https://github.com/Yubico/libfido2)
- [robius-authentication](https://crates.io/crates/robius-authentication) (the crate ikigai-secret's presence gate calls)
- ikigai: [ikigai-secret](https://github.com/ikigai-rs/ikigai-secret/tree/792de1e11326db492fd9ff68a2b58b6d3230aa6a), [ikigai-layers](https://github.com/ikigai-rs/ikigai-layers#readme), [ikigai-cli](https://github.com/ikigai-rs/ikigai-cli#readme), [ikigai-sign](https://github.com/ikigai-rs/ikigai-sign#readme), [ikigai-cms-web](https://github.com/ikigai-rs/ikigai-cms-web#readme)
