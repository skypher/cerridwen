Cerridwen
=========

Cerridwen provides geocentric planetary data that is suitable for
a broad spectrum of astronomical and astrological applications,
with a focus on our solar system. Among its data you will find,
for example, the time of the next sunrise or that of the last full
moon, or simply the current tropical position of the sun.

The motivation for this package is to have a reliable open-source library
and API that provides comprehensive data on various planetary bodies and
factors at a certain point in time.

Cerridwen comes with a simple command-line utility and a JSON server,
but is also designed to serve as a basis for your own application.

This repository hosts the **Rust port**. The original Python package was
retired; the Rust crate under ``rust/`` is API-compatible at the data-shape
level and ships the same CLI and HTTP server, backed by Swiss Ephemeris.

See ``rust/README.md`` for build and usage instructions.

Features
--------

* Get comprehensive data on the sun and moon at almost any point in time
* Rely on the high precision of the NASA JPL ephemeris
* Find planetary events (full moon, squares of planets, retrograde stations, etc.)
* Work with Julian and ISO dates
* Use the JSON HTTP API to integrate from any language
