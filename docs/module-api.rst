Module API
==========

Much of this is still missing docstrings, so this is just
a rough overview.

If you need help, just shoot me a quick mail to <leslie.polzer@gmx.net>
with questions.

Data computation
----------------

These functions take an optional Julian day (defaulting to the current
point in time) and optional longitudes and latitudes, the latter of
which are used for rise/set calculations. You only need to pass
latitude and longitude if you want the function's result to include
rise/set data. If you do so, you must pass both latitude and longitude.

The return value of these functions is an OrderedDict.

.. TODO document some of the data structures returned.

.. module:: cerridwen

.. autofunction:: compute_sun_data

.. autofunction:: compute_moon_data

Date utilities
--------------

These functions provide Julian day conversions and printable output.

.. autofunction:: jd_now

.. autofunction:: iso2jd

.. autofunction:: jd2iso

.. autofunction:: render_pretty_time

.. autofunction:: render_delta_days

