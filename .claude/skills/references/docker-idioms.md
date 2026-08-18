# Docker idiom reference

Condensed, cited reference for writing Dockerfiles and `.dockerignore` in this repository. Prefer a concrete rule over vague advice. This file lives at the `skills/` level, referenced by `writing-dockerfiles` as `../references/docker-idioms.md`. Complements, doesn't duplicate, `fsharp-idioms.md`'s "I/O-layer F#" section and `docs/architecture.md`'s Compute/hosting section — this file owns the Dockerfile/`.dockerignore` content itself, not the F# inside the image or the Azure Container Apps resources that run it (that's `writing-terraform`).

## Multi-stage structure: SDK to build, runtime to ship

Split every Dockerfile into a `build` stage `FROM mcr.microsoft.com/dotnet/sdk:<version>` and a final stage `FROM mcr.microsoft.com/dotnet/aspnet:<version>` (API — needs the ASP.NET Core runtime) or `mcr.microsoft.com/dotnet/runtime:<version>` (worker — no HTTP surface, doesn't need ASP.NET Core's extra layer), copying only `dotnet publish`'s output between them ([Microsoft: Containerize a .NET app](https://learn.microsoft.com/dotnet/core/docker/build-container#create-the-dockerfile); [Microsoft: multi-stage build feature](https://learn.microsoft.com/dotnet/architecture/microservices/docker-application-development-process/docker-app-development-workflow#step-2-create-a-dockerfile-related-to-an-existing-net-base-image)). The SDK image is roughly 800 MB with the compiler and CLI tooling baked in; none of that belongs in what ships. `runtime-deps` is for self-contained publishes only (no managed runtime needed) — this assumes these projects publish framework-dependent (the `aspnet`/`runtime` base images above), which is the default but not yet a recorded decision; confirm framework-dependent vs. self-contained during Stage 11 planning rather than inheriting it silently.

## Base image selection: pin a tag, prefer a hardened variant

Never `FROM ...:latest` — pin the numeric tag (matching the SDK version the `.fsproj` targets) so a build is reproducible and an unrelated upstream bump can't silently change what ships ([Docker docs: pull by digest](https://docs.docker.com/reference/cli/docker/image/pull/#pull-an-image-by-digest-immutable-identifier) makes the same point one level further — pinning by digest, not just tag, for maximum reproducibility; tag-pinning is the practical floor). For the final stage, prefer a chiseled or Alpine variant (`aspnet:<version>-noble-chiseled`, `aspnet:<version>-alpine`) over the default Debian-based tag — smaller (~110 MB vs. several hundred), and the chiseled variant is distroless: no shell, no package manager, nothing an attacker who gets code execution can use to pivot. Trade-off: chiseled images can't `docker exec` into a shell for debugging — if that's needed during Stage 11's local iteration, a non-chiseled base is a reasonable temporary choice, revisited once the image is working.

## Non-root user: `USER $APP_UID`, not a hand-rolled account

Since .NET 8, the official `aspnet`/`runtime` images ship a built-in non-root user and expose it via the `$APP_UID` build arg — add `USER $APP_UID` in the final stage rather than hand-rolling `addgroup`/`adduser` ([Microsoft: Customize containers in Visual Studio](https://learn.microsoft.com/visualstudio/containers/container-build?view=visualstudio#dockerfile-builds-in-visual-studio)). Running as root inside the container is the default if this is omitted — flag its absence. This also changes the exposed port: non-root can't bind privileged ports, so these images default to `8080`/`8081` instead of `80`/`443`; `EXPOSE` and any `ASPNETCORE_URLS` setting must agree with that.

## Layer caching: restore before copying source

Copy project files and run `dotnet restore` before copying the rest of the source tree, then copy source and run `dotnet publish` ([Microsoft: multi-container Docker Compose apps](https://learn.microsoft.com/dotnet/architecture/microservices/multi-container-microservice-net-applications/multi-container-applications-docker-compose) — precompile at build time, never restore/compile at container start). Docker layers are content-hashed and cached top-down; if the project files haven't changed, `restore`'s layer is reused even when source has changed, so an inner-loop rebuild skips re-resolving NuGet packages. Getting the copy order backwards (source before restore) invalidates that cache on every source edit, defeating the point.

**This repo has project references, so "copy only the `.fsproj`" isn't literally just one file.** `Summa.Ledger.Api` references `Summa.Ledger.Domain` and `Summa.Ledger.Store`; `Summa.Ledger.Projections` references the same two. `dotnet restore` needs every `<ProjectReference>` target actually present in the build context to resolve the graph — copying only the entrypoint's own `.fsproj` makes `restore` fail on the first missing reference. Two working options: (a) copy each referenced `.fsproj` individually, preserving their relative directory layout, before restoring; or (b) copy the `.sln` plus every `.fsproj` in the solution and run `dotnet restore <Solution>.sln` — more robust for a multi-project context like this one, since it doesn't require hand-listing the dependency graph per Dockerfile.

## `.dockerignore`: exclude build output and VCS metadata

Without a `.dockerignore`, `docker build`'s context upload includes `bin/`, `obj/`, `.git/`, and anything else in the directory — inflating the build context and risking a stale local `bin/`/`obj/` getting copied into a stage that expects only source. At minimum: `bin/`, `obj/`, `.git/`, `.github/`, `**/*.user`, and any local-only config (`appsettings.Development.json` if it carries secrets). This is the Docker-build-context analog of `.gitignore` — same failure mode (accidentally shipping something local-only) at a different boundary.

## Configuration: environment variables in, never baked in

No connection string, API key, or environment-specific value belongs in a Dockerfile `ENV` instruction or copied-in `appsettings.json` — the image must be identical across dev/staging/prod, with environment-specific values injected at `docker run`/container-app-creation time (`docker run -e ...`, or ACA's secret/env wiring in Stage 13). This is the 12-factor "config in the environment" habit `docs/architecture.md` calls out explicitly as what keeps a later ACA→AKS jump cheap — the image doesn't change, only what's injected around it. The Generic Host convention already in play here (`docs/decisions.md`, 2026-07-30 Worker SDK entry) reads `DOTNET_ENVIRONMENT`; the API reads `ASPNETCORE_ENVIRONMENT` — both are env vars set at run time, never baked into the image.

## Anti-patterns to flag on review

- `FROM ...:latest` anywhere, or an unpinned tag on either stage.
- Running as root in the final stage (missing `USER $APP_UID`).
- `dotnet restore`/`dotnet build` invoked at container *start* (an `ENTRYPOINT`/`CMD` that runs them) instead of at image *build* time.
- Copying full source before `dotnet restore`, defeating layer caching.
- No `.dockerignore`, or one missing `bin/`/`obj/`/`.git/`.
- A secret, connection string, or environment-specific URL hardcoded in `ENV`/`ARG` or a copied-in `appsettings.*.json` rather than injected at run time.
- SDK image (or SDK-installed tooling) present in the final stage — check the last `FROM` in the file, not just that multi-stage syntax was used at all.
- `EXPOSE`/`ASPNETCORE_URLS` still assuming port 80/443 after switching to the non-root user.
