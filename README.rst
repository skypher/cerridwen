Cerridwen
=========

Cerridwen provides data on the moon that is suitable
for both astronomical and astrological purposes. It
comes with a simple command-line utility and a JSON
server.

The motivation for this package is to have a reliable
open-source library and API that provides data on the
moon and, eventually, other planetary bodies at a certain
point in time.


.. contents::
   :depth: 1


Requirements
------------

Cerridwen depends on Python 3. You might be able to make
it work with Python 2 as well. Patches welcome! Please let
me know if there's a version of Python 3 that does not
run Cerridwen properly.

It also depends on these packages:

* pyswisseph, the Python interface to the Swiss Ephemeris library

* numpy, which Cerridwen uses for its ephemeris calculations

* Flask, if you wish to run Cerridwen's API server

These dependencies will be installed automatically as needed.


Quickstart
----------

Installation via pip is very simple. Here are some command
lines to get you started:

::

  pip install cerridwen

This will install Cerridwen and its dependencies. Flask
will be installed when you start ``cerridwen-server`` for the
first time.

To test Cerridwen's data on the console, invoke:

::

  cerridwen

If everything is to your satisfaction you can then
start the API server if you wish:

::

  cerridwen-server

It will start up in the foreground and listen on port 2828,
serving moon data via HTTP in JSON format at the URI ``/v1/moon``.

You can test it as follows:

::

  curl http://localhost:2828/v1/moon

This should give you a proper JSON response with
the current moon data.

Change the listen port by passing the ``-p`` switch to
``cerridwen-server``, followed by the desired port.


FAQ
---

What's the precision of the generated data?
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

For new and full moons (and other angles) the maximum error never exceeds
1/10^6, guaranteed by an assertion.


What zodiac is used for the longitudes?
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

All longitudes whether absolute or relative use the tropical zodiac, i.e. zero
degrees refers to zero degrees tropical Aries, which in turn corresponds to the
sun's position at the vernal equinox of the year in question.


What about other planetary bodies?
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

The source code is designed to be easily extensible to other planets and points.
The goal is to add more planets once the moon interface is reasonably mature.


Will you add more moon data?
^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Yes! For example equatorial latitude, lunation number and rise/set times.


How can I help?
^^^^^^^^^^^^^^^

First and foremost: use it! Also: tell your friends and fellow
astronomers/astrologers!

You can also help write docs, contribute source code and tell me what
you'd like to see in the project.

Donations are also welcome.


Licensing
---------

Cerridwen is distributed under the MIT license. See the file
``LICENSE.txt`` in the source distribution for the full text.

