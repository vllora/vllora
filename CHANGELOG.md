# Changelog

All notable changes to this project will be documented in this file. See [standard-version](https://github.com/conventional-changelog/standard-version) for commit guidelines.

### [0.1.20](https://github.com/vllora/vllora/compare/v0.1.19...v0.1.20) (2025-12-23)


### Features

* Add operation names filter to get traces API ([41e3523](https://github.com/vllora/vllora/commit/41e35230201e72a1395e473759db19c47d3258dc))
* Enhance LLM call handling and tool summary structure ([8283e74](https://github.com/vllora/vllora/commit/8283e7431ab773c1a3bb5753e32568e22701605d))
* Handle MCP functionallity through CLI ([dc9915a](https://github.com/vllora/vllora/commit/dc9915a84841fa428f740c04b3f606042f34011e))
* Remove trace_id from GetLlmCallParams and update related handling ([4a0de09](https://github.com/vllora/vllora/commit/4a0de0940e17ee98f224a0d47cccec807cde0e50))
* Support custom providers, models and endpoints ([#224](https://github.com/vllora/vllora/issues/224)) ([148f9aa](https://github.com/vllora/vllora/commit/148f9aaf36b257bed923853e00ff64beab23dee1))


### Bug Fixes

* Fix response mapping ([c64a13f](https://github.com/vllora/vllora/commit/c64a13fe3c9f99467418799195e4f15d3910dc40))

### [0.1.19](https://github.com/vllora/vllora/compare/v0.1.18...v0.1.19) (2025-12-19)


### Features

* Enhance MCP tools for traces information ([bdd770f](https://github.com/vllora/vllora/commit/bdd770fcc31df6de7abfe8a5929e0450a4ab198b))
* Support tool calls event in responses API ([fbcc376](https://github.com/vllora/vllora/commit/fbcc37611736046ec46ef980acdabb315933ca44))


### Bug Fixes

* Allow to use custom endpoint for openai ([9e44d55](https://github.com/vllora/vllora/commit/9e44d552423dd88398ee0ab61a338c8a151d7797))
* thread cost calculation should be extracted from api_invoke only ([c72c256](https://github.com/vllora/vllora/commit/c72c256ec8bec2ac58f7b5df622639f5d3598a5a))

### [0.1.18](https://github.com/vllora/vllora/compare/v0.1.17...v0.1.18) (2025-12-15)


### Features

* Add custom endpoint support to ProviderInfo ([eb9f43c](https://github.com/vllora/vllora/commit/eb9f43c43f024e387c37297e39e842692ad70201))

### [0.1.17](https://github.com/vllora/vllora/compare/v0.1.16...v0.1.17) (2025-12-12)


### Features

* Integrate responses API ([#215](https://github.com/vllora/vllora/issues/215)) ([00ac1d2](https://github.com/vllora/vllora/commit/00ac1d2abebc325cf12b0dd649b62719f8513a74))

### [0.1.16](https://github.com/vllora/vllora/compare/v0.1.15...v0.1.16) (2025-12-11)


### Bug Fixes

* Paused spans not displayed after page refresh in debug mode ([#210](https://github.com/vllora/vllora/issues/210))
* Cannot collapse content in InputNode (Visual Diagram) ([#212](https://github.com/vllora/vllora/issues/212))
* Cost calculation should only sum from api_invoke spans ([#213](https://github.com/vllora/vllora/issues/213))
* Inconsistent spacing between ThreadList rows ([#214](https://github.com/vllora/vllora/issues/214))
* POST /threads endpoint ignores JSON body pagination parameters ([#216](https://github.com/vllora/vllora/issues/216))
* Infinite scroll fires multiple duplicate requests and uses wrong offset ([#217](https://github.com/vllora/vllora/issues/217))

### [0.1.15](https://github.com/vllora/vllora/compare/v0.1.14...v0.1.15) (2025-12-10)


### Features

* add attributes field to GatewaySpanStartEvent and update related serialization logic ([cdea3e8](https://github.com/vllora/vllora/commit/cdea3e8b513db5518655e0ebcca4cc74c0c43b02))
* add BreakpointReceiverGuard to handle span errors on receiver drop ([ff1af55](https://github.com/vllora/vllora/commit/ff1af55e43b63405e3ba520084b2f5b3834f91c1))
* enhance BreakpointManager to store and manage events by thread_id ([97c4726](https://github.com/vllora/vllora/commit/97c472624b81b6ac715dcb5c40db42337ac90a0d))
* update BreakpointManager to include optional thread_id in requests and responses ([4117bc7](https://github.com/vllora/vllora/commit/4117bc7307617f515b69e5077109f9fcdfdf1336))


### Bug Fixes

* add offset check to prevent out-of-bounds access in TraceService ([38db17e](https://github.com/vllora/vllora/commit/38db17e910f31c5d080c940c32b48f28bf4c3d4e))
* allow too many arguments warning for GatewaySpanStartEvent constructor ([4c31441](https://github.com/vllora/vllora/commit/4c31441d9a0c0a39012f4e3abf5af18abeb8f88f))
* Clear listeners when breakpoints receiver is droped ([4fb248e](https://github.com/vllora/vllora/commit/4fb248ee731044403246d3632a3cc4ebc81994ac))
* Fix usage store in model call span ([df3091c](https://github.com/vllora/vllora/commit/df3091c629afaafed545f5689c3bfeeae9a71c15))

### [0.1.14](https://github.com/vllora/vllora/compare/v0.1.14-prerelease-5...v0.1.14) (2025-12-04)

### [0.1.13](https://github.com/vllora/vllora/compare/v0.1.12...v0.1.13) (2025-12-02)

### [0.1.12](https://github.com/vllora/vllora/compare/v0.1.12-prerelease-11...v0.1.12) (2025-12-02)

### [0.1.8](https://github.com/vllora/vllora/compare/v0.1.9...v0.1.8) (2025-11-21)

### [0.1.7](https://github.com/vllora/vllora/compare/v0.1.7-prerelease-3...v0.1.7) (2025-11-18)


### Bug Fixes

* Change From implementation to TryFrom for GenericGroupResponse and handle errors with GatewayError ([ff34d81](https://github.com/vllora/vllora/commit/ff34d812931d692ff41be1ecd9e6038c06f4e48d))

### [0.1.6](https://github.com/vllora/vllora/compare/v0.1.5-prerelease-1...v0.1.6) (2025-11-04)
