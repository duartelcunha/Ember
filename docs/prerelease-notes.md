# Ember release candidate

Windows evaluation candidate. It is published as a prerelease and excluded from the stable
automatic-update channel: an installed full release is never offered a candidate.

What changed in this version is in `CHANGELOG.md`. What is proven and what is not is in
[the implementation ledger](https://github.com/duartelcunha/Ember/blob/main/docs/production-readiness.md)
and [the native evidence](https://github.com/duartelcunha/Ember/blob/main/docs/native-qualification.md).
A candidate has not been through the native checks that a full release requires.

Updater artifacts are verified against Ember's public key before publication. That signature
is separate from Windows Authenticode: the installer has no Windows publisher certificate and
may show an unknown-publisher warning.

Back up the application configuration before installing a candidate and keep the backup until
the applications you use with Ember have been checked. For an existing installation, run the
installer with `/UPDATE` (or `/S /UPDATE` for a silent upgrade). Do not uninstall the old
version first: older uninstallers can remove configuration and credentials.
