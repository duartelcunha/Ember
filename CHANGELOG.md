# Changelog

## [0.4.0](https://github.com/duartelcunha/Ember/compare/v0.3.0...v0.4.0) (2026-07-10)


### Features

* **settings:** capture shortcut by pressing keys; logo-faithful orb ([92d410d](https://github.com/duartelcunha/Ember/commit/92d410d2f3bae1197b00321da53cdb857dc152f4))

## [0.3.0](https://github.com/duartelcunha/Ember/compare/v0.2.0...v0.3.0) (2026-07-09)


### Features

* **a11y:** overlay screen-reader status + settings hydration skeleton ([f8f5fe6](https://github.com/duartelcunha/Ember/commit/f8f5fe6b25ea685ff48b3467703be04492d22432))
* Add splash screen animation and fix timeout ([a53905c](https://github.com/duartelcunha/Ember/commit/a53905cc9a5d15adaa2a4da6de1f86a2a08345d9))
* **branding:** align accent with the logo orange, drop vite favicon ([fce9e97](https://github.com/duartelcunha/Ember/commit/fce9e97f0ad09f5c546de2eebd9e6426b34ed469))
* **branding:** incandescent star mark, raster brand across the app ([9c1e8f2](https://github.com/duartelcunha/Ember/commit/9c1e8f275e5b36428703a35c94fe46a110cb05c4))
* **context:** merge the focused project's CLAUDE.md into refining ([e6ee58f](https://github.com/duartelcunha/Ember/commit/e6ee58f75123c840529a5f324bd135218c77580a))
* **debug:** add logging, panic hook and in-app diagnostics ([7926a7d](https://github.com/duartelcunha/Ember/commit/7926a7d66752c9495fae2d3e39db8160b3d34f9b))
* **engine:** add the Ember engine around a single LLM call ([464d31a](https://github.com/duartelcunha/Ember/commit/464d31abef8213ed42dc1b983210689cdd62052c))
* **lifecycle:** smooth settings close, animation-driven quit, uninstall cleanup ([c411718](https://github.com/duartelcunha/Ember/commit/c411718d5a0d93ed6b013af0879e19bfe8fd7f22))
* **macos:** working core capture/paste + packaging on macOS ([5f0c28d](https://github.com/duartelcunha/Ember/commit/5f0c28df789516046278c3f89c4a69de7b96c94e))
* **motion:** slower silk animations across overlay, splash and tabs ([3b79d1a](https://github.com/duartelcunha/Ember/commit/3b79d1ad87a2cf18ba98e9c1f15d54b230abbabb))
* **overlay:** metamorphosis star orb tied to the cursor ([bd87f5c](https://github.com/duartelcunha/Ember/commit/bd87f5cfae8aa86a84dfab4e34a5a96ca80dcc46))
* **overlay:** shape-morph orb that reacts to the cursor ([6e78bcd](https://github.com/duartelcunha/Ember/commit/6e78bcdb5bde0b2570c5ebec31b7be0d61ba93c8))
* **providers:** OpenAI-compatible fallback + honest key-store reads ([9bc6547](https://github.com/duartelcunha/Ember/commit/9bc6547e3aeb4309889ff5ff88ebe5119801b9e5))
* **resilience:** pre-validated fallback and honest degrade ([e41d579](https://github.com/duartelcunha/Ember/commit/e41d5799878dd2b859c6513c73e44f4439e5808b))
* **settings:** cream theme, native dark canvas, robust window lifecycle ([5afe7cd](https://github.com/duartelcunha/Ember/commit/5afe7cd46187b143b5c492229dba514f2ebf63a2))
* **settings:** custom seamless title bar, minimize + close only ([a2165f7](https://github.com/duartelcunha/Ember/commit/a2165f7acac6bf65c46a0e487d8d0eaf23380c79))


### Bug Fixes

* **capture:** make refine work in terminals (Windows Terminal) ([6359707](https://github.com/duartelcunha/Ember/commit/6359707d044566cbdd65dc1a221659266d38a940))
* gemini 2.5 streaming issue and update startup animations ([92077f1](https://github.com/duartelcunha/Ember/commit/92077f1aeda4cb77636346d62caec6793e4bd9d0))
* **providers:** default Claude fallback to cheap Haiku tier, not Sonnet ([cd7bce4](https://github.com/duartelcunha/Ember/commit/cd7bce465f08e35767b31345c21c208a66e0a505))
* **settings:** dark canvas so scrollbar and fade match the theme ([48e3b2f](https://github.com/duartelcunha/Ember/commit/48e3b2f0a82f3a28095dceed00e6d721df1ab178))
* **ui:** softer, theme-aware shadows on popovers and the overlay pill ([8af6828](https://github.com/duartelcunha/Ember/commit/8af68288c5fe2f8499dcfedb8d211dc2e266888e))


### Performance Improvements

* **anim:** make every animation compositor-only for smooth 120fps ([b1168e4](https://github.com/duartelcunha/Ember/commit/b1168e414a34e2c8bb2526bc6ad334b73a73f470))

## [0.2.0](https://github.com/duartelcunha/ember/compare/v0.1.2...v0.2.0) (2026-07-03)


### Features

* **core:** add monitor_containing for cursor-based monitor lookup ([72315fb](https://github.com/duartelcunha/ember/commit/72315fb321573c2fcdcb0af54f7d5f7189fa5cce))
* **overlay:** shrink orb, retint to Claude terracotta, swap spinner for a pulsing glow ([d045e8b](https://github.com/duartelcunha/ember/commit/d045e8bca0fa1eef363152bf4de43f28ee8697a8))
* **overlay:** shrink the orb further and tint success to the orb accent ([7f3a174](https://github.com/duartelcunha/ember/commit/7f3a174d413e02939bc1372063bfad11f9b665cd))
* stream refine responses with live progress, cancel, and safe capture ([d06f5a8](https://github.com/duartelcunha/ember/commit/d06f5a871369c5ea41ed1ca58fdf0d84cc69ae0b))


### Bug Fixes

* clamp orb to the monitor under the cursor, not the window's stale monitor ([7e7751c](https://github.com/duartelcunha/ember/commit/7e7751c596284596909c9579717ed0a337097933))
* **core:** make the refiner honest about truncation, language, and untrusted input ([6591a3b](https://github.com/duartelcunha/ember/commit/6591a3b1ce0ef05ec68ba2adfbbbd1d806c2818c))
* **core:** remove em-dash from monitor_containing doc comment ([9efc0ff](https://github.com/duartelcunha/ember/commit/9efc0ffc9161d16ef7fc6a9869f31231880f4af0))
* **core:** sanitize loaded config and back up a corrupt file instead of resetting it ([c59b80d](https://github.com/duartelcunha/ember/commit/c59b80d0d4a0def458f72ac600a2e6db03d74bf3))
* **release:** gate builds on tests and hold prerelease until artifacts upload ([51aefad](https://github.com/duartelcunha/ember/commit/51aefad66b62ccae0bdcd6a50193e9bea44933b9))
* **settings:** resolve state-sync bugs, add key removal, and wire accessible labels ([18fca45](https://github.com/duartelcunha/ember/commit/18fca452fb14622a551ab11e66ca256c1e47479f))
* surface Credential Manager failures and extend the terminal list ([de30be4](https://github.com/duartelcunha/ember/commit/de30be4b030878f897a51555bfacf9bd3b18a9a8))

## [0.1.2](https://github.com/duartelcunha/ember/compare/v0.1.1...v0.1.2) (2026-07-01)


### Bug Fixes

* run release-please and the signed build in one workflow ([#2](https://github.com/duartelcunha/ember/issues/2)) ([c004f63](https://github.com/duartelcunha/ember/commit/c004f633140f91874b8398a1d4c761f98ee7eedc))

## [0.1.1](https://github.com/duartelcunha/ember/compare/v0.1.0...v0.1.1) (2026-07-01)


### Bug Fixes

* keep release tags as plain vX.Y.Z (no component prefix) ([965a96e](https://github.com/duartelcunha/ember/commit/965a96ef5822c64d50a1772af5d0be8bbea56793))
* set releaseName so the release workflow can create a new GitHub release ([01b089f](https://github.com/duartelcunha/ember/commit/01b089f6d256a4e55ed402b8265884df6ebfcee2))
