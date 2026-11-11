# Changelog

## [Unreleased]
- Replace the Elm frontend with TypeScript + React, using tauri-specta generated bindings; Tauri 2.12.5 [PR](https://github.com/ldaIas/oauth-diag-tool/pull/16)
- Add user-based access to OAuth servers [PR](https://github.com/ldaIas/oauth-diag-tool/pull/17)
  - Servers can have users (username, password, user data). Users can be added, edited (password, user data), and deleted from the server card, and created from the OAuth login page
  - New server setting "User-based access". When on, `/authorize` shows a login page (sign in or create a user) instead of the consent page; when off, the consent flow is unchanged. Takes effect on server restart. Users are kept when the setting is turned off
  - User data defaults to `example data for user <username>`
  - `GET /resource` responses include `username` and `userData` (null when the token has no user, e.g. client_credentials). With user-based access on, the response contains only those two fields and no token claims. Access tokens carry a `username` claim, and refresh keeps it
  - The Resource section shows the last user
- Add copy and decode actions to the access token in client responses [PR](https://github.com/ldaIas/oauth-diag-tool/pull/18)
- Collapse server and client config cards to a name row, add list search, and fail the authorization code flow when the browser closes [PR](https://github.com/ldaIas/oauth-diag-tool/pull/19)
- Show metadata fetch errors on the client config form and add a Pull Now button for authorization server metadata [PR](https://github.com/ldaIas/oauth-diag-tool/pull/20)

## [0.0.6] -2026-04-22
- Fix tauri updater to allow auto updates [PR](https://github.com/ldaIas/oauth-diag-tool/pull/15)

## [0.0.5] - 2026-04-22
- Add debug statements for the auto updater [PR](https://github.com/ldaIas/oauth-diag-tool/pull/14)

## [0.0.4] - 2026-04-22
- Bump version as it was missed in last update [PR](https://github.com/ldaIas/oauth-diag-tool/pull/13)

## [0.0.3] - 2026-04-21
- Main add refresh token flow + optimize elm output [PR](https://github.com/ldaIas/oauth-diag-tool/pull/11)


## [0.0.2] - 2026-04-15
- Main tauri.conf.json Config update [PR](https://github.com/ldaIas/oauth-diag-tool/pull/1)
  - **BREAKING CHANGE** - Users will need to re-add clients and servers due to change in application data directory
- Main - Add authorization server metadata call to oauth clients [PR](https://github.com/ldaIas/oauth-diag-tool/pull/2)
- Main OAuth Server Overrides (redirect, access & refresh expiry) [PR](https://github.com/ldaIas/oauth-diag-tool/pull/4)
- UI Label update [PR](https://github.com/ldaIas/oauth-diag-tool/pull/6)
- Prepare for auto-update of application [PR](https://github.com/ldaIas/oauth-diag-tool/pull/7)
- Resource endpoint and token validation [PR](https://github.com/ldaIas/oauth-diag-tool/pull/9)

## [0.0.1] - 2026-04-13
### Added
- Initial release
- OAuth servers support client credentials
- Oauth clients support auth code and client credentials
- Multiple auth server and client configurations