# GitHub Actions on the halfsteady runner

Linux x64 and Android jobs select the `halfsteady-linux-x64` local runner:

```yaml
runs-on: [self-hosted, Linux, X64, halfsteady-linux-x64]
```

The service is registered to the halfsteady organization in the
`homelab-private` runner group. Organization registration was activated on
2026-09-13; the temporary repository registration has been removed.
The [organization verification run](https://github.com/halfsteady/androidemu/actions/runs/34793740024)
passed the desktop tests, build, lint, SDL smoke, AccuracyCoin, and artifact
upload on this group, with hosted platforms skipped.

The host service supplies a fresh non-root container for each job, limited to
4 CPUs and 8 GiB RAM. Workspaces, SDKs, and job credentials disappear when the
container is removed. Build tools, Python, Rust/SDL native dependencies, SSH,
rsync, and the GitHub CLI are in the image; workflows install Java, Rust, and
Android SDK/NDK versions. Jobs cannot use sudo or access the host Docker daemon.

The Android release and Play/site workflows keep their existing triggers and
publishing conditions. A manual Android release build only produces signed
artifacts; it does not publish to Play or create a GitHub release. Tag pushes
retain their existing publication behavior. The upload key and deploy key are
also explicitly removed in cleanup steps. App package names and signing keys
are unchanged by the repository transfer.

Desktop CI runs locally on relevant pushes and same-repository pull requests.
Fork pull requests do not run on this machine. Select `hosted_platforms` when
manually dispatching Desktop portability to additionally test Ubuntu 22.04
x64/ARM64 and macOS Apple Silicon/Intel; these jobs consume hosted minutes.
The ordinary local image uses Ubuntu 24.04, so its binaries have that runtime
baseline. The checks themselves are shared in `desktop-checks.yml`.

One runner handles one job at a time; other jobs queue until its replacement
registers. Do not change a job to `ubuntu-latest` to bypass a queue: that selects
paid/included GitHub-hosted capacity. The runner group admits all private repos
in halfsteady, including newly transferred repos, but each workflow must use
the selector above. Public repositories are excluded. Repositories needing
other tools, container jobs, macOS, ARM64, or hardware devices need another
runner profile.

The source and service runbook live in Controlplaine under
`deploy/github-runner/` and `docs/GITHUB-RUNNERS.md`.

## Repository transfer

The canonical remote is `https://github.com/halfsteady/androidemu.git`:

```sh
git remote set-url origin https://github.com/halfsteady/androidemu.git
```

GitHub redirects old repository links and Git clone/fetch/push URLs. Existing
local directory names and Controlplaine project/task names can stay as they
are. Controlplaine reads `origin` during its normal scan. Do not recreate a
repository at `bsteinfeld/androidemu`, because that would remove the redirect.
GitHub Pages URLs and package registries need separate review; the marketing
site still deploys to `bsteinfeld/emulia-site`, which has not been transferred.
See [GitHub's transfer documentation](https://docs.github.com/en/repositories/creating-and-managing-repositories/transferring-a-repository).
