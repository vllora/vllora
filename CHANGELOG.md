# Changelog

All notable changes to this project will be documented in this file. See [standard-version](https://github.com/conventional-changelog/standard-version) for commit guidelines.

### [0.1.15](https://github.com/langdb/langdb-cloud/compare/v0.1.14...v0.1.15) (2025-12-10)


### Features

* add attributes field to GatewaySpanStartEvent and update related serialization logic ([cdea3e8](https://github.com/langdb/langdb-cloud/commit/cdea3e8b513db5518655e0ebcca4cc74c0c43b02))
* add BreakpointReceiverGuard to handle span errors on receiver drop ([ff1af55](https://github.com/langdb/langdb-cloud/commit/ff1af55e43b63405e3ba520084b2f5b3834f91c1))
* enhance BreakpointManager to store and manage events by thread_id ([97c4726](https://github.com/langdb/langdb-cloud/commit/97c472624b81b6ac715dcb5c40db42337ac90a0d))
* update BreakpointManager to include optional thread_id in requests and responses ([4117bc7](https://github.com/langdb/langdb-cloud/commit/4117bc7307617f515b69e5077109f9fcdfdf1336))


### Bug Fixes

* add offset check to prevent out-of-bounds access in TraceService ([38db17e](https://github.com/langdb/langdb-cloud/commit/38db17e910f31c5d080c940c32b48f28bf4c3d4e))
* allow too many arguments warning for GatewaySpanStartEvent constructor ([4c31441](https://github.com/langdb/langdb-cloud/commit/4c31441d9a0c0a39012f4e3abf5af18abeb8f88f))
* Clear listeners when breakpoints receiver is droped ([4fb248e](https://github.com/langdb/langdb-cloud/commit/4fb248ee731044403246d3632a3cc4ebc81994ac))
* Fix usage store in model call span ([df3091c](https://github.com/langdb/langdb-cloud/commit/df3091c629afaafed545f5689c3bfeeae9a71c15))

### [0.1.14](https://github.com/langdb/langdb-cloud/compare/v0.1.14-prerelease-5...v0.1.14) (2025-12-04)

### [0.1.13](https://github.com/langdb/langdb-cloud/compare/v0.1.12...v0.1.13) (2025-12-02)

### [0.1.12](https://github.com/langdb/langdb-cloud/compare/v0.1.12-prerelease-11...v0.1.12) (2025-12-02)

### [0.1.8](https://github.com/langdb/langdb-cloud/compare/v0.1.9...v0.1.8) (2025-11-21)

### [0.1.7](https://github.com/langdb/langdb-cloud/compare/v0.1.7-prerelease-3...v0.1.7) (2025-11-18)


### Bug Fixes

* Change From implementation to TryFrom for GenericGroupResponse and handle errors with GatewayError ([ff34d81](https://github.com/langdb/langdb-cloud/commit/ff34d812931d692ff41be1ecd9e6038c06f4e48d))

### [0.1.6](https://github.com/langdb/langdb-cloud/compare/v0.1.5-prerelease-1...v0.1.6) (2025-11-04)
