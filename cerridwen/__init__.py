#!/usr/bin/python3

# terminology note: "planet" is used in the astrological sense, i.e.
# also for the sun, moon and asteroids. we also sometimes use
# "planet" when the point in question is something like the AC.

debug_angle_finder = 0

maximum_angle_distance = 1e-6 # our guaranteed maximum error

import swisseph as sweph
import time, calendar, astropy.time
import math
import numpy as np
import collections

import sys
import os

_ROOT = os.path.abspath(os.path.dirname(__file__))
sweph_dir = os.path.join(_ROOT, '../sweph')

sweph.set_ephe_path(sweph_dir)

def jd_now():
    return astropy.time.Time.now().jd

def iso2jd(iso):
    return astropy.time.Time(iso, scale='utc').jd
                        
# TODO: strftime probably is not very reliable
def jd2iso(jd):
    """Convert a Julian date into an ISO 8601 date string representation"""
    return astropy.time.Time(jd, format='jd', scale='utc', precision=0).iso

def parse_jd_or_iso_date(date):
    for format in ['jd', 'iso', 'isot']:
        try:
            return astropy.time.Time(date, format=format, scale='utc').jd
        except ValueError:
            continue
    raise ValueError('Please pass the date as either a Julian Day decimal string ' +
                     '(e.g. "2456799.9897") or as an ISO8601 string denoting a UTC ' +
                     'time in this format: 2014-05-20 23:37:17.')

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

MoonPhaseData = collections.namedtuple('MoonPhaseData', ['trend', 'shape', 'quarter', 'quarter_english'])

# TODO: it would be nice to use recordtype (a mutable version of
# collections.namedtuple) as base class here, but it doesn't work with
# Python 3. See http://bit.ly/1qPmHn0 for the ignored pull request.

class PlanetEvent():
    def __init__(self, description, jd):
        self.description = description
        self.jd = jd

    @property
    def iso_date(self):
        return jd2iso(self.jd)

    @property
    def delta_days(self, rel_jd=None):
        rel_jd = rel_jd or jd_now()
        return self.jd - rel_jd

    def _asdict(self):
        fields = ['description', 'jd', 'iso_date', 'delta_days']
        values = map(lambda name: getattr(self, name), fields)
        return collections.OrderedDict(zip(fields, values))

    def __str__(self):
        return '%s at %s' % (self.description, self.iso_date)

class PlanetLongitude():
    def __init__(self, absolute_degrees):
        super(PlanetLongitude, self).__init__()
        self.absolute_degrees = absolute_degrees

    @property
    def sign(self):
        return signs[int(self.absolute_degrees / 30)]

    @property
    def deg(self):
        return self.absolute_degrees % 30.0

    @property
    def min(self):
        return ((self.deg % 1) * 100) * 60 / 100

    @property
    def rel_tuple(self):
        return (self.sign, self.deg, self.min)

    def _asdict(self):
        fields = ['absolute_degrees', 'sign', 'deg', 'min', 'rel_tuple']
        values = map(lambda name: getattr(self, name), fields)
        return collections.OrderedDict(zip(fields, values))

    def __str__(self):
        sign, deg, minutes = self.rel_tuple
        return '%d %s %d\'' % (deg, sign[:3], minutes)

class Ascendant:
    def __init__(self, long, lat, jd=None):
        jd = jd or jd_now()
        self.jd = jd
        self.long = long
        self.lat = lat

    def name(self):
        return 'Ascendant'

    def __str__(self):
        return '%s at %s' % (self.name(), jd2iso(self.jd))

    def longitude(self, jd=None):
        jd = jd or self.jd
        return sweph.houses(jd, self.lat, self.long)[1][0]

    def position(self, jd=None):
        jd = jd or self.jd
        return PlanetLongitude(self.longitude(jd))

    def sign(self, jd=None):
        jd = jd or self.jd
        return self.position(jd).sign

class Planet:
    def __init__(self, planet_id, jd=None, observer=None):
        jd = jd or jd_now()
        self.id = planet_id
        self.jd = jd
        self.observer = observer

    def name(self):
        return sweph.get_planet_name(self.id)

    def __str__(self):
        return '%s at %s' % (self.name(), jd2iso(self.jd))

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
        return PlanetLongitude(self.longitude(jd))

    def sign(self, jd=None):
        jd = jd or self.jd
        return self.position(jd).sign

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
        # TODO: the link talks about Mercury, what about other planets?
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
        return (180 - mod360_fabs(self.angle(sun, jd), 180)) / 180

    def next_rise(self):
        if self.observer is None:
            raise ValueError('Rise/set times require observer longitude and latitude')
        jd = sweph.rise_trans(self.jd, self.id, self.observer.long, self.observer.lat, rsmi=1)[1][0]
        return PlanetEvent('%s rises' % self.name(), jd)

    def next_set(self):
        if self.observer is None:
            raise ValueError('Rise/set times require observer longitude and latitude')
        jd = sweph.rise_trans(self.jd, self.id, self.observer.long, self.observer.lat, rsmi=2)[1][0]
        return PlanetEvent('%s sets' % self.name(), jd)

    def last_rise(self):
        if self.observer is None:
            raise ValueError('Rise/set times require observer longitude and latitude')
        jd = sweph.rise_trans(self.jd-1, self.id, self.observer.long, self.observer.lat, rsmi=1)[1][0]
        return PlanetEvent('%s rises' % self.name(), jd)

    def last_set(self):
        if self.observer is None:
            raise ValueError('Rise/set times require observer longitude and latitude')
        jd = sweph.rise_trans(self.jd-1, self.id, self.observer.long, self.observer.lat, rsmi=2)[1][0]
        return PlanetEvent('%s sets' % self.name(), jd)

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

        if lookahead >= 0:
            jd_start = jd
            jd_end = jd+lookahead
        else:
            jd_start = jd+lookahead
            jd_end = jd

        next_angles = self.angles_to_planet_within_period(planet, target_angle, jd_start, jd_end)

        if next_angles is None:
            return None

        if lookahead < 0: # backwards search
            next_angles.reverse()

        next_angle_jd = next_angles[0]['jd']

        delta_jd = next_angle_jd - jd
        angle_diff = mod360_fabs(target_angle, next_angles[0]['angle'])

        assert angle_diff <= maximum_angle_distance, (target_angle, next_angles[0]['angle'], angle_diff)

        return (next_angle_jd, delta_jd, angle_diff)

    def angles_to_planet_within_period(self, planet, target_angle, jd_start,
                                       jd_end, sample_interval="auto",
                                       passes=8):
        assert(target_angle<360)
        if sample_interval == "auto":
            sample_interval = 1/20 # days
        if debug_angle_finder:
            print('atpwp (:=%d deg): start=%f (%s), end=%f (%s), interval=%f, '
                  'sample_pass=%d'
                  % (target_angle, jd_start, jd2iso(jd_start), jd_end,
                     jd2iso(jd_end), sample_interval, passes))
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
    def __init__(self, jd=None, observer=None):
        jd = jd or jd_now()
        super(Sun, self).__init__(sweph.SUN, jd, observer)

    def dignity(self, jd=None):
        """Return the dignity of the planet at jd, or None."""
        jd = jd or self.jd
        sign = self.sign(jd)
        if sign == 'Leo':
            return 'rulership'
        elif sign == 'Aries':
            return 'exaltation'
        elif sign == 'Libra':
            return 'detriment'
        elif sign == 'Scorpio':
            return 'Aquarius'
        else:
            return None

class Moon(Planet):
    def __init__(self, jd=None, observer=None):
        jd = jd or jd_now()
        super(Moon, self).__init__(sweph.MOON, jd, observer)

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
        jd = jd or self.jd
        return jd - self.last_new_moon().jd

    def period_length(self, jd=None):
        jd = jd or self.jd
        return self.next_new_moon().jd - self.last_new_moon().jd

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

        return MoonPhaseData._make([trend, shape, quarter, quarter_english])

    def next_new_moon(self, jd=None):
        jd = jd or self.jd
        sun = Planet(sweph.SUN)
        next_angle_jd, delta_jd, angle_diff = self.next_angle_to_planet(sun, 0, jd)
        return PlanetEvent('Upcoming new moon in ' + self.sign(next_angle_jd), next_angle_jd)

    def last_new_moon(self, jd=None):
        jd = jd or self.jd
        sun = Planet(sweph.SUN)
        next_angle_jd, delta_jd, angle_diff = self.next_angle_to_planet(sun, 0, jd, lookahead=-40)
        return PlanetEvent('Preceding new moon in ' + self.sign(next_angle_jd), next_angle_jd)

    def next_full_moon(self, jd=None):
        jd = jd or self.jd
        sun = Planet(sweph.SUN)
        next_angle_jd, delta_jd, angle_diff = self.next_angle_to_planet(sun, 180, jd)
        return PlanetEvent('Upcoming full moon in ' + self.sign(next_angle_jd), next_angle_jd)

    def last_full_moon(self, jd=None):
        jd = jd or self.jd
        sun = Planet(sweph.SUN)
        next_angle_jd, delta_jd, angle_diff = self.next_angle_to_planet(sun, 180, jd, lookahead=-40)
        return PlanetEvent('Preceding full moon in ' + self.sign(next_angle_jd), next_angle_jd)

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

def render_pretty_time(jd):
    """Convert jd into a pretty string representation"""
    year, month, day, hour_frac = sweph.revjul(jd)
    _, hours, minutes, seconds = days_frac_to_dhms(hour_frac/24)
    time_ = calendar.timegm((year,month,day,hours,minutes,seconds,0,0,0))
    return time.strftime('%e %b %Y %H:%M UTC', time.gmtime(time_))

def render_delta_days(delta_days):
    """Convert a time delta into a pretty string representation"""
    days, hours, minutes = days_frac_to_dhms(delta_days)[:3]
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

class LatLong():
    def __init__(self, lat, long):
        if lat > 90 or lat < -90:
            raise ValueError("Latitude must be between -90 and 90")
        if long > 180 or long < -180:
            raise ValueError("Longitude must be between -180 and 180")
        self.lat = lat
        self.long = long

def compute_sun_data(jd=None, observer=None):
    jd = jd or jd_now()

    result = collections.OrderedDict()

    result['jd'] = jd
    result['iso_date'] = jd2iso(jd)

    sun = Sun(jd, observer)

    result['position'] = sun.position()

    result['dignity'] = sun.dignity()

    if observer:
        result['next_rise'] = sun.next_rise()
        result['next_set'] = sun.next_set()
        result['last_rise'] = sun.last_rise()
        result['last_set'] = sun.last_set()

    return result


def compute_moon_data(jd=None, observer=None):
    jd = jd or jd_now()

    result = collections.OrderedDict()

    result['jd'] = jd
    result['iso_date'] = jd2iso(jd)

    moon = Moon(jd, observer)

    result['position'] = moon.position()

    result['phase'] = moon.phase()
    result['illumination'] = moon.illumination()
    result['distance'] = moon.distance()
    result['diameter'] = moon.diameter()
    result['diameter_ratio'] = moon.diameter_ratio()
    result['speed'] = moon.speed()
    result['speed_ratio'] = moon.speed_ratio()
    result['age'] = moon.age()
    result['period_length'] = moon.period_length()
    result['dignity'] = moon.dignity()

    result['next_new_moon'] = moon.next_new_moon()
    result['next_full_moon'] = moon.next_full_moon()
    result['next_new_or_full_moon'] = moon.next_new_or_full_moon()
    result['last_new_moon'] = moon.last_new_moon()
    result['last_full_moon'] = moon.last_full_moon()

    if observer:
        result['next_rise'] = moon.next_rise()
        result['next_set'] = moon.next_set()
        result['last_rise'] = moon.last_rise()
        result['last_set'] = moon.last_set()

    return result

def generate_moon_tables():
    import sqlite3
    conn = sqlite3.connect('moon-events.db')

    moon = Moon()
    # idea sketch: start with previous new moon
    # then go further back, finding all new
    # moons up to a certain date in the past.
    # repeat for the future
    # repeat all this for full moon

def quicktest():
    return # re-enable later when we have a quick sanity test suite.
    print('Cerridwen: running basic sanity tests.')
    import nose
    nose.run()

def main():
    quicktest()

    print('Now:', jd_now())

    print('AC (Berlin): ', Ascendant(13.3, 52.5).position())

    moon = Moon(observer=LatLong(52.5, 13.3))
    # TODO: rise/set tests
    print('moon pos:', moon.position())
    print('next rise:', moon.next_rise())
    print('next set:', moon.next_set())
    print('last rise:', moon.last_rise())
    print('last set:', moon.last_set())
    print(moon.next_new_moon().jd)
    print(moon.last_new_moon())
    print(moon.period_length())

    if debug_angle_finder:
        for i in range(1,100):
            moon = Moon()
            jd = jd_now()+i*30
            new = moon.next_new_moon(jd)
            full = moon.next_full_moon(jd)
            print(jd2iso(new[0]), new[2])
            print(jd2iso(full[0]), full[2])
        sys.exit(1)

# v1.1.0
# use new/full moon tables
# lunation_number
 
# LATER
# latitude: when within band of the sun (David)
# folk_names moon_in_year
# tidal acceleration

# for diameter ratio see the numbers here:
# http://en.wikipedia.org/wiki/Angular_diameter#Use_in_astronomy

# some more ideas:
# * monthly calendar (as widget and for printing)
# * upcoming event stream: https://play.google.com/store/apps/details?id=uk.co.lunarium.iluna

# http://starchild.gsfc.nasa.gov/docs/StarChild/questions/question5.html

# events to subscribe to:
# full, new, 1st quarter, 3rd quarter, sign change, void of course, aspect (one of subset X) to planet (one of subset Y)

if __name__ == '__main__':
    main()
