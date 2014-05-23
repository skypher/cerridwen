Quickstart
----------

Are you hooked by now? ;-)

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


