# Security

## Reporting a vulnerability

Please do not open a public issue for a security problem. Report it through
GitHub's private vulnerability reporting: the "Report a vulnerability" button
under the Security tab of this repository. Only the maintainer sees it.

You will get an answer within a few days. A confirmed problem is fixed in a
release as soon as the fix is ready and noted in the changelog, and you are
credited there if you want to be.

## Supported versions

Only the latest release is supported. Update with `docker compose pull` and
`docker compose up -d`.

## What counts

Anything that lets a user read, change or delete what is not theirs, reach
the server's own network, run script in another user's browser, or get past
the login or the admin role. Problems that need admin access to the instance,
or access to the server itself, are configuration matters rather than
vulnerabilities.
