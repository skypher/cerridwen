#!/usr/bin/python3

import cerridwen
import flask
import time
import collections

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
                #print("cache")
                if (time.time() - v[1]) > self.timeout:
                    raise KeyError
            except KeyError:
                #print("new")
                v = self.cache[key] = f(*args,**kwargs),time.time()
            return v[0]
        func.func_name = f.__name__
        
        return func

def emit_json(result):
    for fieldname in result:
        if isinstance(result[fieldname],
                (cerridwen.PlanetEvent,
                 cerridwen.PlanetLongitude,
                 cerridwen.MoonPhaseData)):
            result[fieldname] = result[fieldname]._asdict()
    import json
    return json.dumps(result, indent=8)


app = flask.Flask('Cerridwen API server')

def make_response(data, status):
    # TODO: if returning an error message, append a newline.
    response = flask.make_response(str(data), status)
    response.headers['Access-Control-Allow-Origin'] = '*'
    if status == 200:
        response.headers['Content-type'] = 'application/json'
    else:
        response.headers['Content-type'] = 'text/plain'
    return response

@app.route("/v1/moon")
@MWT(timeout=10)
def moon_endpoint():
    latlong = None
    jd = cerridwen.jd_now()
    try:
        date = flask.request.args.get('date')
        if date:
            jd = cerridwen.parse_jd_or_iso_date(date)

        lat = flask.request.args.get('latitude')
        if lat:
            lat = float(lat)
        long = flask.request.args.get('longitude')
        if long:
            long = float(long)
        if (long is None and lat is not None) or (lat is None and long is not None):
            raise ValueError("Specify both longitude and latitude or none")
        if lat and long:
            latlong = cerridwen.LatLong(lat, long)
    except ValueError as e:
        return make_response(e, 400)

    result = emit_json(cerridwen.compute_moon_data(jd=jd, observer=latlong))

    return make_response(result, 200)

@app.route("/v1/sun")
def sun_endpoint():
    latlong = None
    jd = cerridwen.jd_now()
    try:
        date = flask.request.args.get('date')
        if date:
            jd = cerridwen.parse_jd_or_iso_date(date)

        lat = flask.request.args.get('latitude')
        if lat:
            lat = float(lat)
        long = flask.request.args.get('longitude')
        if long:
            long = float(long)
        if (long is None and lat is not None) or (lat is None and long is not None):
            raise ValueError("Specify both longitude and latitude or none")
        if lat and long:
            latlong = cerridwen.LatLong(lat, long)
    except ValueError as e:
        return make_response(e, 400)

    result = emit_json(cerridwen.compute_sun_data(jd=jd, observer=latlong))

    return make_response(result, 200)

def start_api_server(port=None, debug=False):
    app.run(port=port, debug=debug)

def main():
    import argparse
    parser = argparse.ArgumentParser(description="Cerridwen API Server")
    parser.add_argument("-p", "--port", type=int, default=2828, 
                        help="Port to listen to")
    parser.add_argument("-t", "--test", action='store_true',
                        help="Print data to stdout for testing")
    parser.add_argument("-d", "--debug", action='store_true',
                        help="Run in debug mode (provides debugger, " +
                             "automatically reloads changed code)")
    args = parser.parse_args()

    print('Running basic sanity tests for Cerridwen...')
    import doctest
    doctest.testmod(cerridwen, raise_on_error=True)
    print('Done.')

    if args.test:
        print(emit_json(cerridwen.compute_moon_data(long=13, lat=52)))
    else:
        print('Starting Cerridwen API server on port %d.' % args.port)
        start_api_server(port=args.port, debug=args.debug)

if __name__ == '__main__':
    main()

