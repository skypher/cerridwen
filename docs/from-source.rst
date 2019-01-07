Setup from source
-----------------

Install dependencies

This can be different for every Unix distribution/OS.

This approach should work most of the time:

::

  pip3 install cerridwen

This will install Cerridwen's release version and its dependencies.

Alternatively, run the following from the toplevel source dir:

::

  pip3 -r requirements.txt

Afterwards, try to run the CLI applications directly from source:

::

  python3 cerridwen/cli.py

This should print basic information.

