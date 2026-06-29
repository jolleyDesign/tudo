# Packaging

How `tudo` is distributed and how to update each channel on a new release.

## Channels

| Channel        | Where                                             | Install command                          |
|----------------|---------------------------------------------------|------------------------------------------|
| Install script | `install.sh` → GitHub Releases                    | `curl -fsSL …/install.sh \| sh`          |
| crates.io      | published from this repo                          | `cargo install tudo`                     |
| Homebrew tap   | `github.com/jolleyDesign/homebrew-tudo`           | `brew install jolleyDesign/tudo/tudo`    |
| AUR (binary)   | `aur.archlinux.org/tudo-bin`                      | `paru -S tudo-bin` (or any AUR helper)   |
| AUR (source)   | `aur.archlinux.org/tudo`                          | `paru -S tudo`                           |

The Homebrew formula and the `tudo-bin` PKGBUILD pin the **release tarballs** and
their **sha256 checksums**, so they must be refreshed every release. The from-source
`tudo` PKGBUILD pins the GitHub source-archive checksum.

## Releasing a new version

1. Bump `version` in `Cargo.toml`; run `cargo build` to refresh `Cargo.lock`.
2. Commit, then tag and push: `git tag -a vX.Y.Z -m vX.Y.Z && git push origin vX.Y.Z`.
   The `release` workflow builds all four target tarballs and attaches them to the release.
3. `cargo publish` (crates.io).
4. Grab the new checksums once the release assets exist:

   ```sh
   V=X.Y.Z
   for t in aarch64-apple-darwin x86_64-apple-darwin \
            x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do
     curl -fsSL "https://github.com/jolleyDesign/tudo/releases/download/v$V/tudo-$t.tar.gz" \
       | sha256sum | sed "s|-|tudo-$t.tar.gz|"
   done
   # source archive (for the from-source AUR PKGBUILD):
   curl -fsSL "https://github.com/jolleyDesign/tudo/archive/refs/tags/v$V.tar.gz" | sha256sum
   ```

5. **Homebrew:** update `version` + the four `sha256` lines in
   `homebrew-tudo/Formula/tudo.rb`, commit, push.
6. **AUR:** in each of `packaging/aur/tudo-bin/` and `packaging/aur/tudo/`, bump
   `pkgver` (reset `pkgrel=1`) and the checksums, then regenerate `.SRCINFO`:

   ```sh
   makepkg --printsrcinfo > .SRCINFO
   ```

   Copy `PKGBUILD` + `.SRCINFO` into the corresponding AUR repo clone and push.
   (The files here are the source of truth; the AUR repos hold only these two files at their root.)
