# Changelog

All notable changes to this project will be documented in this file. See [standard-version](https://github.com/conventional-changelog/standard-version) for commit guidelines.

### [0.1.22](https://github.com/vllora/vllora/compare/v0.1.21...v0.1.22) (2026-01-15)

### [0.1.21](https://github.com/vllora/vllora/compare/v0.1.20...v0.1.21) (2026-01-09)


### Features

* Add distri as component ([2c2f382](https://github.com/vllora/vllora/commit/2c2f3823209855275ad2bb5c1a795894ba2e5d94))
* Add Lucy configuration management for agents ([f89df0e](https://github.com/vllora/vllora/commit/f89df0e938b86eb52ac8ea68a0033571b815c56e))
* Add OTLP metrics port configuration ([4d150c4](https://github.com/vllora/vllora/commit/4d150c480f270b324ce51f040f8c4f57433eea24))
* Add project_slug support to VlloraMcp and related services ([84d5082](https://github.com/vllora/vllora/commit/84d50828cd84bb6b4dcdce726426663a8eb83730))
* Add support for distri agents ([f79a1a2](https://github.com/vllora/vllora/commit/f79a1a252b12c7e176a70ff85f35f04e987643fb))
* Add totals to run overview ([f6b8fa5](https://github.com/vllora/vllora/commit/f6b8fa59b87f142d7704237596a586dc54ed482c))
* Dont block main thread for distri download and startup ([911c328](https://github.com/vllora/vllora/commit/911c3284f49216157162553853c06f229fea3108))
* Enhance Distri client functionality and configuration management ([57cbad1](https://github.com/vllora/vllora/commit/57cbad113201e7e22ddd5774c765397bde45c796))
* Enhance SQLite connection handling in DbPool ([c5694ad](https://github.com/vllora/vllora/commit/c5694ad1c37468a748109a77a2293aea97d64148))
* Implement agent registration with detailed status reporting ([d742b20](https://github.com/vllora/vllora/commit/d742b20876dfaac0071ab9bd75cbef5d5ad87038))
* Implement Lucy project handling and middleware integration ([2d1dd1d](https://github.com/vllora/vllora/commit/2d1dd1d229f348e9221ac25c12c3f758cefabc3e))
* Support metrics in telemetery ([#241](https://github.com/vllora/vllora/issues/241)) ([3212952](https://github.com/vllora/vllora/commit/3212952aca365c5445316b2962fd4c85ba006d68))
* Update distri based on current version and latest release ([c5e990c](https://github.com/vllora/vllora/commit/c5e990cf4e45e149c9f842ab423c8c6702cc7b6d))


### Bug Fixes

* Add missing property to api_invoke span ([92bbaba](https://github.com/vllora/vllora/commit/92bbaba6eead7a1dea6e3e4cb267804663700b09))
* Fix API key usage for lucy ([6fd6a12](https://github.com/vllora/vllora/commit/6fd6a12be29c73540e90312a38ad89ee6865fc2e))
* Fix download of distri-server ([5d0eed6](https://github.com/vllora/vllora/commit/5d0eed6792f2261e8b16dbc51496645cb73ca374))
* Fix run span timing ([66c7807](https://github.com/vllora/vllora/commit/66c7807b4babebc39c721bea3950d56f0af359b7))
* getSpanContent return data ([53763fc](https://github.com/vllora/vllora/commit/53763fcbc364366123b295d5931a97a8dd9c1e25))
* Increase default limit for overview pagination from 100 to 1000 ([ef941f4](https://github.com/vllora/vllora/commit/ef941f4cea6a02bb150f683fdc54122e16b6c977))
* Integrate KeyStorageError into GatewayApiError and update key retrieval in chat completion executors ([39c94a6](https://github.com/vllora/vllora/commit/39c94a6c2d7ca18732f9e7f230fe015ae7a991b9))
* Update provider info handling in model metadata ([90dcf62](https://github.com/vllora/vllora/commit/90dcf621d4fe2599a5a1a5c608928e601d501ad0))
* Use correct project in events stream ([db39467](https://github.com/vllora/vllora/commit/db3946700fd7ecfe48e97d51b8a2a0beaeb3aec3))
* Use project slug for credentials retrieve ([bc6c8dc](https://github.com/vllora/vllora/commit/bc6c8dc9854948ef05a6d8cb0de0f70e10b9b577))
* Use server port args from cli when no command is defined ([af6df7d](https://github.com/vllora/vllora/commit/af6df7d97a383c3b10565ee22f8167bba790d8eb))
* when clone request in lucy project ([738b62d](https://github.com/vllora/vllora/commit/738b62d87b63b91d572007ea04a9d31f4891add5))

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
