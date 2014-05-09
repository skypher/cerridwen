#!/usr/bin/python3

import cerridwen
import flask

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
    def json_api():
        result = emit_json(cerridwen.compute_moon_data())
        status = 200
        response = flask.make_response(result, status)
        response.headers['Access-Control-Allow-Origin'] = '*'
        response.headers['Content-type'] = 'text/json'
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

