.. Cerridwen documentation master file, created by
   sphinx-quickstart on Tue May 20 20:48:56 2014.
   You can adapt this file completely to your liking, but it should at least
   contain the root `toctree` directive.

Welcome to Cerridwen's documentation!
=====================================

Contents:

.. toctree::
   :maxdepth: 2

.. include:: ../README.rst

FAQ
---

What's the precision of the generated data?
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

For new and full moons (and other angles) the maximum error allowed by
Cerridwen's calculations never exceeds 10\ :sup:`-6`\  degrees, guaranteed
by an assertion. This translates to 0.0036 arc seconds. The target precision
of the underlying ephemeris library is 0.001 arc seconds.

But please note that the current implementation of the API server uses
memoization, generating a new response every 10 seconds only due to
`efficiency considerations`__.

__ efficiency_


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

Donations are also welcome.



Indices and tables
==================

* :ref:`genindex`
* :ref:`modindex`
* :ref:`search`

