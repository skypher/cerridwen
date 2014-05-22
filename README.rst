Cerridwen
=========

Cerridwen provides data on the moon that is suitable for both astronomical
and astrological purposes. It comes with a simple command-line utility and
a JSON server, but is also designed to serve as a basis for your own
application.

The motivation for this package is to have a reliable open-source library
and API that provides data on the moon and, eventually, other planetary
bodies at a certain point in time.

You can see a demo of the JSON API at this address:

::

  http://cerridwen.viridian-project.de/api/v1/moon

The current implementation caches data for 10 seconds.  Please let me know
if you intend to use this for more than testing.


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


Licensing
---------

Cerridwen is distributed under the MIT license. See the file
``LICENSE.txt`` in the source distribution for the full text.

