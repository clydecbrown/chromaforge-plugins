# Tools

Test stand-ins for the programs plug-ins hand files to (e.g. `tools/fake-<program>/`): small native binaries
that behave like the real program as far as a plug-in can tell (watch the same folder, write the same output
names), so the round trip runs in CI without the real program. Workspace members, `publish = false`, never
released.
