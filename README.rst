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


Features
--------

* Get comprehensive data on the sun and moon at almost any point in time

* Rely on the high precision of the NASA JPL ephemeris

* Make use of a powerful numpy-based algorithm for planetary event
  finding, (e.g. full moon, squares of planets etc.)

* Work with Julian and ISO dates

* Use Cerridwen's JSON API to ensure loose coupling

* Extend Cerridwen with ease to suit your own needs


Documentation
-------------

Cerridwen's full documentation can be found at http://cerridwen.readthedocs.org/


Stability
---------

Cerridwen includes a basic test suite for its calculations and API.

At this time the API is reasonably stable but may change without
warning. Please let me know when you need better API stability for
your project.

Also, you can check out the status of the current development version
at Travis CI:

.. image:: https://travis-ci.org/skypher/cerridwen.svg?branch=master
    :target: https://travis-ci.org/skypher/cerridwen

