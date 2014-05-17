#!/usr/bin/python3

import cerridwen
import flask
import time

# http://code.activestate.com/recipes/325905-memoize-decorator-with-timeout/
class MWT(object):
    """Memoize With Timeout"""
    _caches = {}
    _timeouts = {}
    
    def __init__(self,timeout=2):
        self.timeout = timeout
        
    def collect(self):
        """Clear cache of results which have timed out"""
        for func in self._caches:
            cache = {}
            for key in self._caches[func]:
                if (time.time() - self._caches[func][key][1]) < self._timeouts[func]:
                    cache[key] = self._caches[func][key]
            self._caches[func] = cache
    
    def __call__(self, f):
        self.cache = self._caches[f] = {}
        self._timeouts[f] = self.timeout
        
        def func(*args, **kwargs):
            kw = sorted(kwargs.items())
            key = (args, tuple(kw))
            try:
                v = self.cache[key]
                print("cache")
                if (time.time() - v[1]) > self.timeout:
                    raise KeyError
            except KeyError:
                print("new")
                v = self.cache[key] = f(*args,**kwargs),time.time()
            return v[0]
        func.func_name = f.__name__
        
        return func


app = flask.Flask('Cerridwen API server')

def emit_json(result):
    # Note: simplejson treats namedtuples as dicts by default but this is
    # one dep less.
    for field in ['position', 'sun', 'phase', 'next_new_moon', 'next_full_moon', 'next_new_or_full_moon']:
        result[field] = result[field]._asdict()
    import json
    return json.dumps(result, indent=8)

def start_api_server(port):
    @app.route("/v1/moon")

    @MWT(timeout=10)
    def json_api():
        result = emit_json(cerridwen.compute_moon_data())
        status = 200
        response = flask.make_response(result, status)
        response.headers['Access-Control-Allow-Origin'] = '*'
        response.headers['Content-type'] = 'text/json'
        #response.headers['Cache-Control'] = 'public, max-age=10, s-maxage=10'
        #expiry_time = datetime.datetime.utcnow() + datetime.timedelta(0,10)
        #response.headers['Expires'] = expiry_time.strftime("%a, %d %b %Y %H:%M:%S GMT")
        #last_modified_time = datetime.datetime.utcnow() + datetime.timedelta(0,10)
        #response.headers['Last-Modified'] = expiry_time.strftime("%a, %d %b %Y %H:%M:%S GMT")
        return response

    app.debug = True
    app.run(port=port)

def main():
    import argparse
    parser = argparse.ArgumentParser(description="Cerridwen API Server")
    parser.add_argument("-p", "--port", type=int, default=2828, 
                        help="Port to listen to")
    args = parser.parse_args()

    print('Running basic sanity tests for Cerridwen...')
    import doctest
    doctest.testmod(cerridwen, raise_on_error=True)
    print('Done.')

    print('Starting Cerridwen API server on port %d.' % args.port)
    start_api_server(port=args.port)

if __name__ == '__main__':
    main()

