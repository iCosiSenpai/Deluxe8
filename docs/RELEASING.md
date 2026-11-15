# Releasing and code signing

A release is the Windows installer (`Deluxe8_<version>_x64-setup.exe`, Compat engine included) plus `SHA256SUMS.txt`, and,
once validated, the Linux AppImage and deb. The Native engine and DLSS are not part of a release: the launcher downloads
them on the player's PC (pinned and verified, see [ARCHITECTURE.md](ARCHITECTURE.md#components-downloaded-by-the-launcher)).

## Two ways to build one

| | Where | Signing |
|---|---|---|
| `git tag v0.2.0 && git push --tags` | GitHub Actions, [`release.yml`](../.github/workflows/release.yml): Compat engine from source, launcher, installer, checksums, build provenance, **draft** release | Azure Artifact Signing when the repository is configured for it (below), otherwise unsigned with a warning |
| `pwsh tools/release.ps1 -Thumbprint <sha1>` | this PC, into `out/release/v<version>` | any code-signing certificate in the Windows certificate store, through `signtool` (engine, launcher, uninstaller, installer) |

Every executable is signed, not only the installer: the launcher starts `deluxe8-engine.exe` and `deluxe8-room.exe`, and
unsigned executables started from a user folder are what antivirus heuristics look at first.

## Why sign, and what signing does not do

Windows shows *"Windows protected your PC"* (SmartScreen) for downloads without a reputation, and some antivirus engines
flag unknown unsigned programs. A signature from a publicly trusted certificate:

- names the publisher in the UAC prompt and in the file's properties, and proves the file was not changed after signing;
- lets SmartScreen build **reputation for the publisher** instead of for each file hash.

It does **not** remove the SmartScreen prompt on day one: Microsoft's documentation for its own signing service says the
prompt stops "once the file hash has sufficient download history". Keep the same certificate from release to release so
the reputation carries over, and submit new releases to Microsoft (below).

## Getting a certificate

Since June 2023 every publicly trusted code-signing key must live in hardware (a token or a cloud HSM): there are no more
`.pfx` files to put in CI. The realistic options for Deluxe8:

| Option | Who can get it | Publisher shown | Cost | Works with |
|---|---|---|---|---|
| **Azure Artifact Signing** (formerly Trusted Signing) | organisations in the EU, UK, US, Canada, Australia and a few more; **individuals only in the US and Canada** | the validated legal name | paid Azure subscription + monthly plan | `release.yml` (secrets below) |
| **Open-source code signing certificate** from a public CA, e.g. Certum's for open-source developers (key on a token or in Certum's SimplySign cloud) | individuals, after identity checks | your legal name | yearly fee | `tools/release.ps1 -Thumbprint` on your PC |
| **SignPath Foundation** | open-source projects that meet its conditions (OSI licence, already released, built in CI, reviewed by SignPath) | "SignPath Foundation" | free | a SignPath step in CI (not wired yet: apply first) |

Notes:

- The certificate holder is a real person or company: an individual certificate puts **your name** on every file.
- SignPath's conditions include signing only binaries built from your own sources; the Compat engine is a fork of Eden,
  which needs their review. Ask before relying on it.
- A self-signed certificate is useless for this: Windows trusts it only on PCs where you installed it yourself.

### Azure Artifact Signing in GitHub Actions

1. Create the Artifact Signing account, an identity validation and a *Public Trust* certificate profile (Azure portal).
2. Create an app registration (service principal) and give it the *Artifact Signing Certificate Profile Signer* role on
   the account.
3. In the GitHub repository settings:
   - **Secrets**: `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID`
   - **Variables**: `AZURE_SIGNING_ENDPOINT` (e.g. `https://weu.codesigning.azure.net`), `AZURE_SIGNING_ACCOUNT`,
     `AZURE_SIGNING_PROFILE`
4. Push a tag. `release.yml` installs `artifact-signing-cli`, signs the engine executables, gives Tauri a `signCommand`
   for the launcher, uninstaller and installer, and fails if any of them is not validly signed.

### A certificate on your PC

Install the certificate's software (e.g. SimplySign Desktop, or the USB token's driver) so the certificate appears in
*certmgr.msc → Personal → Certificates*, copy its thumbprint, then:

```powershell
pwsh tools/release.ps1 -Thumbprint 0123456789ABCDEF0123456789ABCDEF01234567
# -TimestampUrl http://time.certum.pl   to use your CA's timestamp server (default: DigiCert's)
# -SkipEngine                            to reuse out/engine/win-x64
```

The script signs `deluxe8-engine.exe` and `deluxe8-room.exe`, builds the installer with Tauri's built-in signing
(`certificateThumbprint`, SHA-256, RFC 3161 timestamp), checks every signature and writes `SHA256SUMS.txt`. Upload the
files to the draft release by hand (`gh release upload v<version> out/release/v<version>/*`).

## Antivirus false positives

If an engine flags a release anyway:

1. Check the file on [VirusTotal](https://www.virustotal.com) by its SHA-256 (search, do not upload private builds).
2. Submit it to Microsoft as a software developer: [microsoft.com/wdsi/filesubmission](https://www.microsoft.com/wdsi/filesubmission).
   Do this for every new release while the reputation is young: it also helps SmartScreen.
3. Report the false positive to the other vendors that flag it (each has a form).

Things that keep the heuristics calm, already in place: no packers (UPX and friends), complete version information in
every executable (publisher, product, copyright), a standard NSIS installer, per-user install without elevation, and
downloads that are pinned and verified.

## Release checklist

- [ ] `cargo test` (launcher, d8-nx), `pnpm check`, CI green.
- [ ] Version bumped in `apps/launcher/src-tauri/tauri.conf.json`, `apps/launcher/package.json`, `Cargo.toml`.
- [ ] Engine smoke test (AGENT.md, *Testing*) with the installed build: Compat and Native, one race each.
- [ ] Every executable signed (`Get-AuthenticodeSignature`), `SHA256SUMS.txt` matches.
- [ ] Release notes: what changed, known issues, the SHA-256 of the installer.
- [ ] Publish the draft; submit the installer to Microsoft (above).
