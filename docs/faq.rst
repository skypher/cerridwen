FAQ
===

.. contents::
   :local:

What zodiac is used for the longitudes?
---------------------------------------

All longitudes whether absolute or relative are based on the tropical
zodiac.  In this system of reference zero degrees refers to zero degrees
tropical Aries, which in turn corresponds to the sun's position at the
vernal equinox of the year in question.


What about other planetary bodies?
----------------------------------

Cerridwen's source code is designed to be easily extensible to other
planets and points. The goal is to add more planets in the future,
probably starting with Mercury.


Will you add more moon data?
----------------------------

Yes! For example equatorial latitude and lunation numbers.


What's the precision of the generated data?
-------------------------------------------

Please see the documentation on :doc:`precision`.


Hey, some of this stuff is slow!
--------------------------------

You're right! At the moment the new and full moons are computed anew
everytime, which is hard on CPU power. This will change radically with
the next version of the module which will have a separate lookup table
generation stage for these and other events. This will also pave the
way for certain new features like the lunation number.


How can I help?
---------------

First and foremost: use it! Also: tell your friends and fellow
astronomers/astrologers!

You can also help write docs, contribute source code and tell me what
you'd like to see in the project.

Donations are also welcome, they help me eat and pay my rent! :-)
Even 1$ helps.

