#!/usr/bin/python3

# license: GPL3

# terminology note: "planet" is used in the astrological sense, i.e.
# also for the sun, moon and asteroids.

debug_angle_finder = 0

maximum_angle_distance = 1e-6 # our guaranteed precision

import swisseph as sweph
import time, calendar
import math
import numpy as np
import collections

import sys

sweph.set_ephe_path('/home/sky/eph/sweph')

def jd_now():
    gmtime = time.gmtime()
    return sweph.julday(gmtime.tm_year,
                        gmtime.tm_mon,
                        gmtime.tm_mday,
                        gmtime.tm_hour+((gmtime.tm_min * 100 / 60) / 100))

def mod360_fabs(a, b):
    """fabs for a,b in mod(360)"""
    # Surprisingly enough there doesn't seem to be a more elegant way.
    # Check http://stackoverflow.com/questions/6192825/
    a %= 360
    b %= 360

    if a < b:
        return mod360_fabs(b, a)
    else:
        return min(a-b, b-a+360)

signs = ['Aries','Taurus','Gemini','Cancer','Leo','Virgo',
         'Libra','Scorpio','Sagittarius','Capricorn','Aquarius','Pisces'];

traditional_major_aspects = [0, 60, 90, 120, 180, 270, 300]

class Planet:
    def __init__(self, planet_id, jd=jd_now()):
        self.id = planet_id
        self.jd = jd
        self.Position = collections.namedtuple('PlanetPosition',
                ['sign', 'degrees', 'minutes', 'absolute_degrees'])
        self.Event = collections.namedtuple('PlanetEvent', ['jd', 'delta_jd', 'angle_diff', 'description'])

    def name(self):
        return sweph.get_planet_name(self.id)

    def diameter(self, jd=None):
        """The apparent diameter of the planet, in arc minutes."""
        jd = jd or self.jd
        return sweph.pheno_ut(jd, self.id)[3] * 60

    def longitude(self, jd=None):
        jd = jd or self.jd
        long = sweph.calc_ut(jd, self.id)[0]
        return long

    def distance(self, jd=None):
        jd = jd or self.jd
        distance = sweph.calc_ut(jd, self.id)[2]
        return distance

    def position(self, jd=None):
        jd = jd or self.jd
        long = sweph.calc_ut(jd, self.id)[0]
        sign = signs[int(long / 30)]
        reldeg = long % 30.0
        minutes = ((reldeg % 1) * 100) * 60 / 100
        return self.Position._make([sign, reldeg, minutes, long])

    def sign(self, jd=None):
        jd = jd or self.jd
        return self.position(jd)[0]

    def speed(self, jd=None):
        jd = jd or self.jd
        speed = sweph.calc_ut(jd, self.id)[3]
        return speed

    def is_rx(self, jd=None):
        jd = jd or self.jd
        speed = self.speed(jd)
        return speed < 0

    def is_stationing(self, jd=None):
        # http://houseofdaedalus.blogspot.de/2012/07/meaning-of-retrograde-motion.html
        # TODO: this is for Mercury, what about other planets?
        jd = jd or self.jd
        speed = self.speed()
        return math.fabs(speed) < 0.2

    def angle(self, planet, jd=None):
        jd = jd or self.jd
        return (self.longitude(jd) - planet.longitude(jd)) % 360

    def illumination(self, jd=None):
        # TODO also return an indicator of whether it is growing or shrinking.
        jd = jd or self.jd
        sun = Planet(sweph.SUN)
        print(mod360_fabs(self.angle(sun, jd), 180))
        return (180 - mod360_fabs(self.angle(sun, jd), 180)) / 180

    def last_rise(self):
        raise NotImplementedError

    def next_rise(self):
        raise NotImplementedError

    def last_set(self):
        raise NotImplementedError

    def next_set(self):
        raise NotImplementedError

    def next_angle_to_planet(self, planet, target_angle, jd=None,
                             orb="auto", lookahead="auto"):
        jd = jd or self.jd
        """Return (jd, delta_jd) indicating the time of the next target_angle
        to a planet.
        Return None if no result could be found in the requested lookahead
        period."""
        # TODO: set lookahead, sampling_interval and orb according to the speed
        #       of planets involved, if "auto".
        # TODO: honor orb
        assert(target_angle<360)
        if lookahead == "auto":
            lookahead = 40 # days
        next_angles = self.angles_to_planet_within_period(planet, target_angle, jd, jd+lookahead)
        if next_angles:
            next_angle_jd = next_angles[0]['jd']
            delta_jd = next_angle_jd - jd
            angle_diff = mod360_fabs(target_angle, next_angles[0]['angle'])
            assert angle_diff <= maximum_angle_distance, (target_angle, next_angles[0]['angle'], angle_diff)
            return (next_angle_jd, delta_jd, angle_diff)
        else:
            return None

    def angles_to_planet_within_period(self, planet, target_angle, jd_start,
                                       jd_end, sample_interval="auto",
                                       passes=8):
        assert(target_angle<360)
        if sample_interval == "auto":
            sample_interval = 1/20 # days
        if debug_angle_finder:
            print('atpwp (:=%d deg): start=%f (%s), end=%f (%s), interval=%f, '
                  'sample_pass=%d'
                  % (target_angle, jd_start, format_jd(jd_start), jd_end,
                     format_jd(jd_end), sample_interval, passes))
        jds = np.arange(jd_start, jd_end, sample_interval)
        def angle_at_jd(d):
            return self.angle(planet, d)
        angle_at_jd_v = np.vectorize(angle_at_jd)
        angles = angle_at_jd_v(jds)
        if debug_angle_finder:
            print("The angles:",angles[0], angles[-1])
        target_adjusted_angles = (angles - target_angle) % 360
        gradient_signs = np.sign(np.diff(target_adjusted_angles))
        sign_changes = np.roll(np.diff(gradient_signs) != 0, 1)
        matching_jds = jds[sign_changes]

        if matching_jds.size < 2:
            return None

        matches = []
        jd_starts = matching_jds[::2]
        jd_ends = matching_jds[1::2]
        # sometimes we have an odd number of sign changes;
        # in that case just ignore the last one.
        start_end_pairs = min(jd_starts.size, jd_ends.size)
        for i in range(start_end_pairs):
            jd_start = jd_starts[i]
            jd_end = jd_ends[i]
            match = {'jd_start':jd_start, 'jd_end':jd_end,
                     'angle_start': angle_at_jd(jd_start),
                     'angle_end': angle_at_jd(jd_end)}
            if debug_angle_finder:
                print('match:', match)
            matches.append(match);

        def match_mean(match):
            jd_mean = (match['jd_start'] + match['jd_end']) / 2
            angle_mean = angle_at_jd(jd_mean)
            #print(match,angle_mean)
            return {'jd': jd_mean, 'angle': angle_at_jd(jd_mean)}

        refined_matches = []
        if passes:
            for match in matches:
                new_sample_interval = sample_interval * (1/100)
                result = self.angles_to_planet_within_period(planet,
                        target_angle,
                        match['jd_start']-new_sample_interval*100,
                        match['jd_end']+new_sample_interval*100,
                        new_sample_interval,
                        passes-1)
                if result:
                    refined_matches += result
                else:
                    if debug_angle_finder:
                        print('Notice: stopping angle finder with %d passes '
                              'remaining.' % (passes-1))
                    refined_matches.append(match_mean(match))
        else:
            for match in matches:
                refined_matches.append(match_mean(match))

        return refined_matches

    def next_sign_change(self, jd=None):
        # TODO
        jd = jd or self.jd
        return jd

    def time_left_in_sign(self, jd=None):
        # TODO
        jd = jd or self.jd
        return jd

class Sun(Planet):
    def __init__(self, jd=jd_now()):
        super(Sun, self).__init__(sweph.SUN, jd)

class Moon(Planet):
    def __init__(self, jd=jd_now()):
        super(Moon, self).__init__(sweph.MOON, jd)

    def speed_ratio(self, jd=None):
        # 11.6deg/d to 14.8deg/d
        jd = jd or self.jd
        return (self.speed(jd) - 11.6) / 3.2

    def diameter_ratio(self, jd=None):
        # 29.3' to 34.1'
        jd = jd or self.jd
        return (self.diameter(jd) - 29.3) / 4.8

    def dignity(self, jd=None):
        """Return the dignity of the planet at jd, or None."""
        jd = jd or self.jd
        sign = self.sign(jd)
        if sign == 'Cancer':
            return 'rulership'
        elif sign == 'Taurus':
            return 'exaltation'
        elif sign == 'Capricorn':
            return 'detriment'
        elif sign == 'Scorpio':
            return 'fall'
        else:
            return None

    def age(self, jd=None):
        raise NotImplementedError
        jd = jd or self.jd
        #return jd - jd_last_new_moon

    def period_length(self, jd=None):
        raise NotImplementedError
        jd = jd or self.jd
        #return jd_next_new_moon - jd_last_new_moon

    def phase(self, jd=None):
        jd = jd or self.jd
        sun = Planet(sweph.SUN)
        angle = self.angle(sun, jd)

        quarter = None
        quarter_english = None
        if angle > 350 or angle < 10:
            quarter = 0
        elif 80 < angle < 100:
            quarter = 1
        elif 170 < angle < 190:
            quarter = 2
        elif 260 < angle < 290:
            quarter = 3

        if quarter is not None:
            quarter_names = ["new", "first quarter", "full", "third quarter"]
            quarter_english = quarter_names[quarter]

        if 0 < angle < 90:
            trend = 'waxing'
            shape = 'crescent'
        elif 90 <= angle < 180:
            trend = 'waxing'
            shape = 'gibbous'
        elif 190 <= angle < 270:
            trend = 'waning'
            shape = 'gibbous'
        else:
            trend = 'waning'
            shape = 'crescent'

        MoonPhaseData = collections.namedtuple('MoonPhaseData',
                ['trend', 'shape', 'quarter', 'quarter_english'])
        return MoonPhaseData._make([trend, shape, quarter, quarter_english])

    def last_new_moon(self, jd=None):
        # TODO
        raise NotImplementedError

    def next_new_moon(self, jd=None):
        """
        >>> math.floor(Moon(2456720.24305).next_new_moon()[0])
        2456747
        >>> math.floor(Moon(2456731.375).next_new_moon()[0] * 1e6)
        2456747281033
        """
        jd = jd or self.jd
        sun = Planet(sweph.SUN)
        next_angle_jd, delta_jd, angle_diff = self.next_angle_to_planet(sun, 0, jd)
        return self.Event._make([next_angle_jd, delta_jd, angle_diff, 'New Moon in ' + self.sign(next_angle_jd)])

    def next_full_moon(self, jd=None):
        """
        >>> math.floor(Moon(2456731.376389).next_full_moon()[0] * 1e6)
        2456733214114
        """
        jd = jd or self.jd
        sun = Planet(sweph.SUN)
        next_angle_jd, delta_jd, angle_diff = self.next_angle_to_planet(sun, 180, jd)
        return self.Event._make([next_angle_jd, delta_jd, angle_diff, 'Full Moon in ' + self.sign(next_angle_jd)])

    def next_new_or_full_moon(self, jd=None):
        # TODO optimize
        next_new_moon = self.next_new_moon(jd)
        next_full_moon = self.next_full_moon(jd)
        if next_new_moon.jd < next_full_moon.jd:
            return next_new_moon
        else:
            return next_full_moon

    def is_void_of_course(self, jd=None):
        """Whether the moon is void of course at a certain point in time.
        Returns a tuple (boolean, float) indicating whether it is void
        of course and up to which point in time."""
        # as per http://www.astrologyweekly.com/astrology-articles/void-of-course-moon.php
        # and http://www.estelledaniels.com/articles/VoidMoon.html
        # the traditional planets plus the major new ones (uranus, neptune, pluto) are used
        # plus the traditional aspects of conjunction, sextile, square, trine, opposition
        raise NotImplementedError
        jd = jd or self.jd
        return (False, jd) # TODO

    def lunation_number(self):
        # TODO http://en.wikipedia.org/wiki/Lunation_Number
        raise NotImplementedError
        jd = jd or self.jd
        return 0

def format_jd(jd):
    """Convert jd into an ISO 8601 string representation"""
    year, month, day, hour_frac = sweph.revjul(jd)
    _, hours, minutes, seconds = days_frac_to_dhms(hour_frac/24)
    time_ = calendar.timegm((year,month,day,hours,minutes,seconds,0,0,0))
    return time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(time_))

def days_frac_to_dhms(days_frac):
    """Convert a day float to integer days, hours and minutes.

    Returns a tuple (days, hours, minutes).
    
    >>> days_frac_to_dhms(2.5305)
    (2, 12, 43, 55)
    """
    days = math.floor(days_frac)
    hms_frac = days_frac - days
    hours = math.floor(hms_frac * 24)
    minutes_frac = hms_frac - hours / 24
    minutes = math.floor(minutes_frac * 1440)
    seconds = math.floor((minutes_frac - minutes / 1440) * 86400)

    return (days, hours, minutes, seconds)

def render_delta_jd(delta_jd):
    days, hours, minutes = days_frac_to_dhms(delta_jd)[:3]
    result = [] 

    if days > 0:
        result += ['%d days' % days]

    if hours > 0:
        result += ['%d hours' % hours]

    if days == 0 and minutes > 0:
        result += ['%d minutes' % minutes]

    if days == 0 and hours == 0 and minutes == 0:
        result = ['less than a minute']

    return ' '.join(result);

def build_result_dict(jd=jd_now()):
    result = collections.OrderedDict()

    result['jd'] = jd
    result['utc'] = format_jd(jd)

    moon = Moon(jd)
    result['moon'] = moon.position()

    sun = Sun(jd)
    result['sun'] = sun.position()

    result['phase'] = moon.phase()
    result['illumination'] = moon.illumination()
    result['distance'] = moon.distance()
    result['diameter'] = moon.diameter()
    result['diameter_ratio'] = moon.diameter_ratio()
    result['speed'] = moon.speed()
    result['speed_ratio'] = moon.speed_ratio()
    result['next_new_moon'] = moon.next_new_moon()
    result['next_full_moon'] = moon.next_full_moon()
    result['next_new_full_moon'] = moon.next_new_or_full_moon()
    #result['next_new_moon']['utc'] = format_jd(result['next_new_moon']['jd'])
    #result['next_full_moon']['utc'] = format_jd(result['next_full_moon']['jd'])
    #result['next_new_full_moon']['utc'] = format_jd(result['next_new_full_moon']['jd'])

    result['dignity'] = moon.dignity()

    return result


def emit_text(result):
    # TODO build string and return
    print('Julian day:', result['jd'])
    print('Universal time (UTC):', result['utc'])
    print('Local time:', time.asctime())

    sign, deg, minutes = result['moon'][:3]
    print('Moon: %d %s %d\'' % (deg, sign[:3], minutes))

    sign, deg, minutes = result['sun'][:3]
    print('Sun: %d %s %d\'' % (deg, sign[:3], minutes))

    trend, shape, quarter, quarter_english = result['phase']
    phase = trend + ' ' + shape
    print("phase: %s, quarter: %s, illum: %d%%" %
            (phase, quarter_english, result['illumination'] * 100))

    next_new_moon = result['next_new_moon']
    print("next new moon: in %s (%s / %f)" %
            (render_delta_jd(next_new_moon.delta_jd), format_jd(next_new_moon.jd), next_new_moon.jd))

    next_full_moon = result['next_full_moon']
    print("next full moon: in %s (%s / %f)" %
            (render_delta_jd(next_full_moon.delta_jd), format_jd(next_full_moon.jd), next_full_moon.jd))

def emit_json(result):
    # Note: simplejson treats namedtuples as dicts by default but this is
    # one dep less.
    for field in ['moon', 'sun', 'phase', 'next_new_moon', 'next_full_moon']:
        result[field] = result[field]._asdict()
    import json
    return json.dumps(result, indent=8)

def emit_html_widget(result, options={}):
    import flask
    return flask.render_template('widget.html', data=result)

def generate_moon_tables():
    raise NotImplementedError

    import sqlite3
    conn = sqlite3.connect('moon.db')

    moon = Moon()
    # idea sketch: start with previous new moon
    # then go further back, finding all new
    # moons up to a certain date in the past.
    # repeat for the future
    # repeat all this for full moon

def start_api_server():
    import flask
    app = flask.Flask(__name__, template_folder='.')
    app.jinja_env.filters['render_delta_jd'] = render_delta_jd

    @app.route("/")
    def test():
        return '<html><body><iframe src="/html" width=300 height=200/></body></html>'

    @app.route("/html")
    def html_widget():
        result = emit_html_widget(build_result_dict())
        status = 200
        response = flask.make_response(result, status)
        response.headers['Content-type'] = 'text/html'
        return response

    @app.route("/json")
    def json_api():
        result = emit_json(build_result_dict())
        status = 200
        response = flask.make_response(result, status)
        response.headers['Content-type'] = 'text/json'
        return response

    app.debug = True
    app.run()


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("-s", "--server", default=False, action="store_true",
                        help="Run as API server")
    args = parser.parse_args()
    server = args.server

    print('Running basic sanity tests.')
    import doctest
    doctest.testmod()
    print('Done.')

    if server:
        print('Starting API server.')
        start_api_server()
    else:
        if debug_angle_finder:
            for i in range(1,100):
                moon = Moon()
                jd = jd_now()+i*30
                new = moon.next_new_moon(jd)
                full = moon.next_full_moon(jd)
                print(format_jd(new[0]), new[2])
                print(format_jd(full[0]), full[2])
            sys.exit(1)

        result = build_result_dict()

        emit_text(result);
        emit_json(result);


# v1
# rise, set (angle 0 to ac) -- last and next
# new/full moon position

# v1.1.0
# last_new/last_full
# use new/full moon tables
# jd should be part of the Planet instance
 
# LATER
# latitude: when within band of the sun (David)
# last_new last_full folk_names moon_in_year age period_length
# lunation_number

# for diameter ratio see the numbers here:
# http://en.wikipedia.org/wiki/Angular_diameter#Use_in_astronomy

# some more ideas:
# * monthly calendar (as widget and for printing)
# * upcoming event stream: https://play.google.com/store/apps/details?id=uk.co.lunarium.iluna

# http://starchild.gsfc.nasa.gov/docs/StarChild/questions/question5.html
