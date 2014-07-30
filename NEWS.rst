Only major releases are documented here.

1.4.0
=====
* Add planets Mercury, Venus, Mars, Jupiter, Saturn.
  Their interface is yet incomplete though.

* Add new Planet methods max_speed, mean_orbital_period,
  relative_orbital_velocity, average_motion_per_year,
  aspect_lookahead, default_sample_interval.

* Add sign change detection via Planet.next_sign_change().

* Add Moon.last_new_or_full_moon()

* Precision lowered to 0.0072 arc seconds (was 0.0036).

* Update code for astropy 0.4 (rewrote one test case).

* Various bugfixes.


1.3.0
=====
* Add arc seconds to relative position

* Add right ascension, declination and ecliptical latitude

* Refurbish cli.py


1.2.0
=====

* Use astropy for time conversions

* Vast documentation update

* Extend test suite

* Remove sun data from moon endpoint response


1.1.0
=====

* Swiss Ephemeris data files are now included in the package

* Use nose instead of doctest for quick sanity tests

* Add a lot of functions (e.g. rise/set times)

* cerridwen-server: new switch --test/-t for quick testing

* Various minor amendments and changes

* New sun data computation function and API endpoint


1.0.0
=====

Initial release.
