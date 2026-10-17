# Isolated entitlement preview evidence

[Simulated preview video](SIMULATED-entitlement-preview.mp4) shows a test-only component at
384×812. Pending, error and revoked states retain an enabled recovery control; purchase/restore
are blocked during pending. Tips remain disabled. The component is not mounted by the app.

This is not a Play checkout, Android purchase, backend verification, real Pro grant or physical
TV recovery. The recovery callback in this preview only increments a test counter. Separately,
core mock-ADB tests exercise real existing re-enable/emergency recovery implementations after
downgrade, and a browser test exercises the actual More screen's Free recovery path.

Reproduce from mobile:

```sh
ENTITLEMENT_VIDEO_DIR=/tmp/entitlement-video node --test --test-name-pattern='simulated UI' tests/entitlement-preview.test.mjs
```

Playwright WebM was transcoded to H.264 MP4 without changing the test. The recorded controls
are a development presentation, not a finalized purchase UI or evidence of store readiness.
