# Deploying aci.mask.dev

The website is static and lives in `site/`.

## Website

Import this repository into Vercel and attach the custom domain
`aci.mask.dev`. The root `vercel.json` rewrites `/` to `site/index.html` and
serves `/install.sh` from `site/install.sh`.

After Vercel creates the project, point the `aci.mask.dev` DNS record at the
target Vercel provides for the domain.

## Install Script

The public install command is:

```sh
curl -fsSL https://aci.mask.dev/install.sh | sh
```

The script downloads the latest GitHub Release asset for the current platform
and installs `aci` into `$HOME/.local/bin` by default. Override the destination
with:

```sh
ACI_INSTALL_DIR=/usr/local/bin curl -fsSL https://aci.mask.dev/install.sh | sh
```

Install a specific release by setting `ACI_VERSION`:

```sh
ACI_VERSION=v0.1.1 curl -fsSL https://aci.mask.dev/install.sh | sh
```

Supported release assets:

- `aci-aarch64-apple-darwin.tar.gz`
- `aci-x86_64-apple-darwin.tar.gz`
- `aci-x86_64-unknown-linux-gnu.tar.gz`

## Release

Create and push a version tag to publish binaries:

```sh
git tag v0.1.0
git push origin v0.1.0
```

The release workflow builds the platform archives and publishes them to the
GitHub Release. Once that workflow finishes, the install script can download the
new version.
