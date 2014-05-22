.. Cerridwen documentation master file, created by
   sphinx-quickstart on Tue May 20 20:48:56 2014.
   You can adapt this file completely to your liking, but it should at least
   contain the root `toctree` directive.

.. include:: ../README.rst
   :end-before: Documentation


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


FAQ
---

.. toctree::
   :maxdepth: 2

What's the precision of the generated data?
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

For new and full moons (and other angles) the maximum error allowed by
Cerridwen's calculations never exceeds 10\ :sup:`-6`\  degrees, guaranteed
by an assertion. This translates to 0.0036 arc seconds. The target precision
of the underlying ephemeris library is even better, at 0.001 arc seconds.
Thus Cerridwen's calculations are precise enough to get event times of
the ascendant and of rise and set times down to the correct second.

But please note that the current implementation of the API server uses
memoization for the moon data, generating a new response every 10 seconds
only due to `efficiency considerations`__. You can easily turn this off
if you run your own API server, or just wait for the next version of
Cerridwen that will be able to calculate new and full moons in a more
efficient manner.

__ efficiency_


What zodiac is used for the longitudes?
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

All longitudes whether absolute or relative are based on the tropical
zodiac.  In this system of reference zero degrees refers to zero degrees
tropical Aries, which in turn corresponds to the sun's position at the
vernal equinox of the year in question.


What about other planetary bodies?
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Cerridwen's source code is designed to be easily extensible to other
planets and points. The goal is to add more planets in the future,
probably starting with Mercury.


Will you add more moon data?
^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Yes! For example equatorial latitude and lunation numbers.


.. _efficiency:

Hey, this stuff is slow!
^^^^^^^^^^^^^^^^^^^^^^^^

You're right! At the moment the new and full moons are computed anew
everytime, which is hard on CPU power. This will change radically with
the next version of the module which will have a separate lookup table
generation stage for these and other events. This will also pave the
way for certain new features like the lunation number.


How can I help?
^^^^^^^^^^^^^^^

First and foremost: use it! Also: tell your friends and fellow
astronomers/astrologers!

You can also help write docs, contribute source code and tell me what
you'd like to see in the project.

Donations are also welcome, they help me eat and pay my rent! :-)
Even 1$ helps.


Contributing
------------

Cerridwen's codebase is on GitHub, at `skypher/cerridwen`_.

.. _skypher/cerridwen: https://github.com/skypher/cerridwen

Feel free to browse, fork and submit patches and bug reports.

Feature requests are also welcome!

If you need help, you can also write to me at <leslie.polzer@gmx.net>.


Licensing
---------

Cerridwen is distributed under the MIT license:

.. include:: ../LICENSE.txt
   :literal:


Changelog
---------

.. include:: ../NEWS.rst


Indices and tables
==================

* :ref:`genindex`
* :ref:`modindex`
* :ref:`search`

