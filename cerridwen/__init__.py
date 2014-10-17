#!/usr/bin/python3

# terminology note: "planet" is used in the astrological sense, i.e.
# also for the sun, moon and asteroids. we also sometimes use
# "planet" when the point in question is something like the AC.

import swisseph as sweph
import time, calendar, astropy.time
import math
import numpy as np
import collections

import sys
import os

from cerridwen.approximate import approximate_event_date, debug_event_approximation, maximum_error

_ROOT = os.path.abspath(os.path.dirname(__file__))
sweph_dir = os.path.join(_ROOT, '../sweph')
dbfile = os.path.join(_ROOT, 'events.db')

sweph.set_ephe_path(sweph_dir)


signs = ['Aries','Taurus','Gemini','Cancer','Leo','Virgo',
         'Libra','Scorpio','Sagittarius','Capricorn','Aquarius','Pisces'];

# TODO there are quite a few minor aspects that aren't included yet.
dexter_aspects = [
        (30, 'semi-sextile'),
        (45, 'semi-square'),
        (60, 'sextile'),
        (72, 'quintile'),
        (120, 'trine'),
        (144, 'bi-quintile'),
        (150, 'quincunx')
]

sinister_aspects = reversed([(360 - x[0], x[1]) for x in dexter_aspects])

aspects = (
        [(0, 'conjunction', None)] +
        [x + ('dexter',) for x in dexter_aspects] +
        [(180, 'opposition', None)] +
        [x + ('sinister',) for x in sinister_aspects]
)

def angle_to_aspect_name(angle):
    return [aspect[1] for aspect in aspects if aspect[0] == angle]

def aspect_name_to_angle(name):
    "Get the (dexter, if applicable) angle for an aspect name"
    if name == 'conjunction':
        return 0
    if name == 'opposition':
        return 180
    else:
        return [aspect[0] for aspect in dexter_aspects if aspect[1] == name]
                   
traditional_major_aspects = ['conjunction', 'sextile', 'square', 'trine', 'opposition']
sign_related_aspects = ['conjunction', 'semi-sextile', 'sextile', 'square', 'trine', 'quincunx', 'opposition']

def jd_now():
    return astropy.time.Time.now().jd

def iso2jd(iso):
    return astropy.time.Time(iso, scale='utc').jd
                        
def jd2iso(jd):
    """Convert a Julian date into an ISO 8601 date string representation"""
    return astropy.time.Time(jd, format='jd', scale='utc', precision=0).iso

def parse_jd_or_iso_date(date):
    for format in ['jd', 'iso', 'isot']:
        try:
            return astropy.time.Time(float(date) if format == 'jd' else date,
                    format=format, scale='utc').jd
        except ValueError:
            continue
    raise ValueError('Please pass the date as either a Julian Day decimal string ' +
                     '(e.g. "2456799.9897") or as an ISO8601 string denoting a UTC ' +
                     'time in this format: 2014-05-20 23:37:17.')

def mod360_distance(a, b):
    """distance between a and b in mod(360)"""
    # Surprisingly enough there doesn't seem to be a more elegant way.
    # Check http://stackoverflow.com/questions/6192825/
    a %= 360
    b %= 360

    if a < b:
        return mod360_distance(b, a)
    else:
        return min(a-b, b-a+360)

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
        if rel_jd is None: rel_jd = jd_now()
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
        return (self.deg % 1) * 60

    @property
    def sec(self):
        return ((self.deg % 1) * 60 - math.floor(self.min)) * 60

    @property
    def rel_tuple(self):
        """Return a tuple with fixed order consisting of sign, degrees,
        arc minutes and seconds, with the latter three being truncated
        (or rounded down) integers.
        
        This is basically a convenience function for printing."""
        return (self.sign,
                math.floor(self.deg),
                math.floor(self.min),
                math.floor(self.sec))

    def _asdict(self):
        fields = ['absolute_degrees', 'sign', 'deg', 'min', 'sec', 'rel_tuple']
        values = map(lambda name: getattr(self, name), fields)
        return collections.OrderedDict(zip(fields, values))

    def __str__(self):
        sign, deg, min, sec = self.rel_tuple
        return '%d %s %d\' %d"' % (deg, sign[:3], min, sec)

class Ascendant:
    def __init__(self, long, lat, jd=None):
        if jd is None: jd = jd_now()
        self.jd = jd
        self.long = long
        self.lat = lat

    def name(self):
        return 'Ascendant'

    def __str__(self):
        return '%s at %s' % (self.name(), jd2iso(self.jd))

    def longitude(self, jd=None):
        if jd is None: jd = self.jd
        return sweph.houses(jd, self.lat, self.long)[1][0]

    def position(self, jd=None):
        if jd is None: jd = self.jd
        return PlanetLongitude(self.longitude(jd))

    def sign(self, jd=None):
        if jd is None: jd = self.jd
        return self.position(jd).sign

class FixedZodiacPoint:
    def __init__(self, degrees):
        self.degrees = degrees

    def name(self):
        return 'Fixed zodiac point at %f degrees (%s)' % (self.degrees, self.position)

    def __str__(self):
        return '%s' % self.name()

    def longitude(self, jd=None):
        return self.degrees

    def position(self, jd=None):
        return PlanetLongitude(self.longitude())

    def sign(self, jd=None):
        return self.position().sign

    def max_speed(self):
        return 0

    def aspect_possible(self, planet, angle):
        return True # TODO only if we don't aspect another FixedZodiacPoint

    def aspect_lookahead(self):
        return 10**10 # "infinity"

class Planet:
    def __init__(self, planet_id, jd=None, observer=None):
        if jd is None: jd = jd_now()
        self.id = planet_id
        self.jd = jd
        self.observer = observer

    def name(self):
        return sweph.get_planet_name(self.id)

    def __str__(self):
        return '%s at %s' % (self.name(), jd2iso(self.jd))

    def diameter(self, jd=None):
        """The apparent diameter of the planet, in arc minutes."""
        if jd is None: jd = self.jd
        return sweph.pheno_ut(jd, self.id)[3] * 60

    def longitude(self, jd=None):
        "Ecliptical longitude of planet"
        if jd is None: jd = self.jd
        long = sweph.calc_ut(jd, self.id, sweph.FLG_SWIEPH)[0]
        return long

    def latitude(self, jd=None):
        "Ecliptical latitude of planet"
        if jd is None: jd = self.jd
        lat = sweph.calc_ut(jd, self.id, sweph.FLG_SWIEPH)[1]
        return lat

    def rectascension(self, jd=None):
        if jd is None: jd = self.jd
        flags = sweph.FLG_SWIEPH + sweph.FLG_EQUATORIAL
        ra = sweph.calc_ut(jd, self.id, flags)[0]
        return ra

    def declination(self, jd=None):
        if jd is None: jd = self.jd
        flags = sweph.FLG_SWIEPH + sweph.FLG_EQUATORIAL
        dec = sweph.calc_ut(jd, self.id, flags)[1]
        return dec

    def distance(self, jd=None):
        if jd is None: jd = self.jd
        distance = sweph.calc_ut(jd, self.id, sweph.FLG_SWIEPH)[2]
        return distance

    def position(self, jd=None):
        if jd is None: jd = self.jd
        return PlanetLongitude(self.longitude(jd))

    def sign(self, jd=None):
        if jd is None: jd = self.jd
        return self.position(jd).sign

    def speed(self, jd=None):
        if jd is None: jd = self.jd
        speed = sweph.calc_ut(jd, self.id)[3]
        return speed

    def max_speed(self):
        raise NotImplementedError

    def is_rx(self, jd=None):
        if jd is None: jd = self.jd
        speed = self.speed(jd)
        return speed < 0

    def is_stationing(self, jd=None):
        # http://houseofdaedalus.blogspot.de/2012/07/meaning-of-retrograde-motion.html
        # TODO: the link talks about Mercury, what about other planets?
        if jd is None: jd = self.jd
        speed = self.speed()
        return math.fabs(speed) < 0.2

    def angle(self, planet, jd=None):
        if jd is None: jd = self.jd
        return (self.longitude(jd) - planet.longitude(jd)) % 360

    def illumination(self, jd=None):
        # TODO also return an indicator of whether it is growing or shrinking.
        if jd is None: jd = self.jd
        sun = Sun()
        return (180 - mod360_distance(self.angle(sun, jd), 180)) / 180

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

    def aspect_possible(self, planet, angle):
        return True


    def next_angle_to_planet(self, planet, target_angle, jd=None,
                             orb="auto", lookahead="auto", sample_interval="auto",
                             passes="auto"):
        if jd is None: jd = self.jd
        """Return (jd, delta_jd) indicating the time of the next target_angle
        to a planet.
        Return None if no result could be found in the requested lookahead
        period."""
        assert(target_angle<360)

        #if self.max_speed() < planet.max_speed():
        #    raise ValueError('Target planet must move slower than primary planet ' +
        #                     'or undefined behavior will result')

        if lookahead == "auto":
            lookahead = min(self.aspect_lookahead(), planet.aspect_lookahead())

        if lookahead >= 0:
            jd_start = jd
            jd_end = jd+lookahead
        else:
            jd_start = jd+lookahead
            jd_end = jd

        next_angles = self.angles_to_planet_within_period(planet, target_angle,
                                                          jd_start, jd_end,
                                                          sample_interval=sample_interval,
                                                          passes=passes,
                                                          orb=orb)

        if not next_angles:
            return None

        if lookahead < 0: # backwards search
            next_angles.reverse()

        next_angle_jd = next_angles[0]['jd']

        delta_jd = next_angle_jd - jd
        angle_diff = mod360_distance(target_angle, next_angles[0]['angle'])

        assert angle_diff <= maximum_error, (target_angle, next_angles[0]['angle'], angle_diff)

        return (next_angle_jd, delta_jd, angle_diff)


    def angles_to_planet_within_period(self, planet, target_angle, jd_start,
                                       jd_end, sample_interval="auto",
                                       passes="auto", orb="auto"):
        # TODO let user specify precision and whether only the first match is
        # interesting. then limit the number of passes accordingly.
        # TODO: set orb according to the planets involved, if "auto".
        # TODO this function does not support angles between planets at different
        # points in time. Consider this.
        assert(target_angle<360)

        if passes == "auto":
            passes = 8
        if sample_interval == "auto":
            sample_interval = self.default_sample_interval()
        if orb == "auto":
            orb = maximum_error * 10 # "exact"

        assert(orb > 0 and orb < 360)

        def find_local_minima(jds):
            def angle_at_jd(d):
                return self.angle(planet, d)
            angle_at_jd_v = np.vectorize(angle_at_jd)
            angles = angle_at_jd_v(jds)
            if debug_event_approximation:
                print("The angles: %f,%f,...,%f,%f (%d total):" %
                        (angles[0], angles[1], angles[-2], angles[-1], angles.size))
            target_adjusted_angles = (angles - target_angle) % 360

            distances = np.vectorize(mod360_distance)(180, target_adjusted_angles)
            distances -= 180
            distances *= -1

            distances_gradient = np.diff(distances)
            is_extremum = np.roll(np.diff(np.sign(distances_gradient)), 1) != 0
            curves_left = np.roll(np.diff(distances_gradient), 1) > 0
            is_minimum = np.logical_and(is_extremum, curves_left)

            ### PLOTTING SKETCH ###
            #for i in range(0, len(gradient_signs)):
            #    print(jds[i], jd2iso(jds[i]), target_adjusted_angles[i], gradient_signs[i])

            #import matplotlib.pyplot as plt
            #print(len(jds),len(target_adjusted_angles), len(np.diff(target_adjusted_angles)), len(sign_changes))
            #min_elems = min(len(jds),len(target_adjusted_angles), len(np.diff(target_adjusted_angles)), len(sign_changes), len(distances_g2),
            #        len(distances_gradient_signs_gradient))
            #filename = "%s-%s-pass%d.png" % (self, planet, passes)
            #plt.plot(jds[:-3], distances[:-3], jds[:-3], distances_gradient[:-2], jds[:-3], distances_g2[:-1])
            #plt.plot(jds[:-3], distances_g2[:-1])
            #plt.plot(jds[:min_elems], distances[:min_elems],
            #         jds[:min_elems], distances_gradient_signs_gradient[:min_elems],
            #         jds[:min_elems], distances_g2[:min_elems])
            #for i in range(0, len(distances_g2)):
            #    if distances_gradient_signs_gradient[i] != 0 and distances_g2[i] > 0:
            #        print('found local minimum:',
            #                jds[i], distances[i], distances_gradient[i],
            #                distances_gradient_signs_gradient[i],
            #                distances_g2[i])
            #plt.ylim(-0.5,.5)
            #plt.savefig(filename)

            if debug_event_approximation:
                for i in range(0, len(curves_left)):
                    if is_minimum[i]:
                        print('found local minimum:',
                                jds[i], distances[i],
                                distances_gradient[i],
                                is_extremum[i],
                                curves_left[i],
                                is_minimum[i]
                                )
                if is_minimum.size == 0:
                    print('no local minimum found')
                else:
                    print(jds[is_minimum])

            if is_minimum.size == 0:
                return None

            matching_jds = jds[is_minimum]
            matches = dict(zip(matching_jds, angle_at_jd_v(matching_jds)))
            return [matches, angle_at_jd]

        def is_inside_orb(angle):
            return mod360_distance(angle, target_angle) <= orb;

        events = approximate_event_date(jd_start, jd_end, find_local_minima, is_inside_orb,
                                        distance_function=mod360_distance,
                                        sample_interval=sample_interval, passes=passes)

        result = []
        for jd, value in events.items():
            result.append({'jd':jd, 'angle':value})

        return sorted(result, key=lambda event: event['jd'])

    def retrogrades_within_period(self, jd_start, jd_end, sample_interval="auto", passes="auto"):
        if passes == "auto":
            passes = 8
        if sample_interval == "auto":
            sample_interval = self.default_sample_interval()

        def find_retrograde_turn(jds):
            def speed_at_jd(d):
                return self.speed(d)
            speed_at_jd_v = np.vectorize(speed_at_jd)
            speeds = speed_at_jd_v(jds)
            if debug_event_approximation:
                print("The speeds: %f,%f,...,%f,%f (%d total):" %
                        (speeds[0], speeds[1], speeds[-2], speeds[-1], speeds.size))

            is_zero_crossing = np.roll(np.diff(np.sign(speeds)), 1) != 0

            if is_zero_crossing.size == 0:
                return None

            matching_jds = jds[is_zero_crossing]
            matches = dict(zip(matching_jds, speed_at_jd_v(matching_jds)))
            return [matches, speed_at_jd]

        events = approximate_event_date(jd_start, jd_end, find_retrograde_turn, lambda x: True,
                                        distance_function=lambda a,b: math.fabs(a - b),
                                        sample_interval=sample_interval, passes=passes)

        result = []
        for jd, speed in events.items():
            type = 'direct' if speed > 0 else 'rx' 
            result.append({'jd':jd, 'speed':speed, 'type': type})

        return sorted(result, key=lambda event: event['jd'])

    def next_rx_event(self, jd=None, lookahead='auto'):
        # TODO implement support for stationing event
        # TODO be smarter about lookahead
        assert(not(isinstance(self, (Sun, Moon))))

        if jd is None: jd = self.jd

        if lookahead == "auto":
            lookahead = self.aspect_lookahead()

        if lookahead >= 0:
            jd_start = jd
            jd_end = jd+lookahead
        else:
            jd_start = jd+lookahead
            jd_end = jd

        rx_events = self.retrogrades_within_period(jd_start, jd_end)

        if not rx_events:
            return None

        if lookahead < 0: # backwards search
            rx_events.reverse()

        next_rx_event_jd = rx_events[0]['jd']

        delta_jd = next_rx_event_jd - jd
        speed_zero_distance = math.fabs(rx_events[0]['speed'])

        assert speed_zero_distance <= maximum_error, (rx_events[0]['speed'], speed_zero_distance)

        return {'jd': next_rx_event_jd, 'type': rx_events[0]['type']}

    def mean_orbital_period(self):
        raise NotImplementedError

    def relative_orbital_velocity(self):
        """Orbital velocity, relative to Earth's."""
        raise NotImplementedError

    def average_motion_per_year(self):
        """Average motion per year in degrees.
        cf. http://www.auxmaillesgodefroy.com/planet_speeds"""
        raise NotImplementedError

    def aspect_lookahead(self):
        # TODO depends on aspect
        raise NotImplementedError

    def default_sample_interval(self):
        return 1 / (self.max_speed() * 3)

    def sign_change_lookahead(self):
        raise NotImplementedError

    def next_sign_change(self, jd=None):
        if jd is None: jd = self.jd
        next_sign_idx = (signs.index(self.sign(jd)) + 1) % 12
        planet = FixedZodiacPoint(next_sign_idx * 30)
        result_jd = self.next_angle_to_planet(planet, 0, jd, lookahead=self.sign_change_lookahead())
        assert(result_jd is not None)
        # we nudge the result a bit to the right to make sure it's in the
        # new sign. otherwise functions like time_left_in_sign get confused.
        return result_jd[0] + maximum_error

    def time_left_in_sign(self, jd=None):
        if jd is None: jd = self.jd
        return self.next_sign_change(jd) - jd

    def next_event(self, evtypes='all'):
        # evtypes: all, rise, set, new, full,
        # traditional_major_aspects = [0, 60, 90, 120, 180, 270, 300]
        # semi-sextile and more
        # to planets: traditional_planets = sun, moon, mercury, venus, mars, jupiter, saturn
        # extra: chiron, neptune, uranus, pluto, ceres, pallas
        raise NotImplementedError

class Sun(Planet):
    def __init__(self, jd=None, observer=None):
        if jd is None: jd = jd_now()
        super(Sun, self).__init__(sweph.SUN, jd, observer)

    def max_speed(self):
        return 1.0197676

    def dignity(self, jd=None):
        """Return the dignity of the planet at jd, or None."""
        if jd is None: jd = self.jd
        sign = self.sign(jd)
        if sign == 'Leo':
            return 'rulership'
        elif sign == 'Aries':
            return 'exaltation'
        elif sign == 'Libra':
            return 'detriment'
        elif sign == 'Aquarius':
            return 'fall'
        else:
            return None

    def sign_change_lookahead(self):
        return 35

    def aspect_lookahead(self):
        return 365 * 3.5 # roughly max time to conjunction/opposition with Mars

    def average_motion_per_year(self):
        return 360

    def mean_orbital_period(self):
        # http://hpiers.obspm.fr/eop-pc/models/constants.html
        return 365.256363004

class Moon(Planet):
    def __init__(self, jd=None, observer=None):
        if jd is None: jd = jd_now()
        super(Moon, self).__init__(sweph.MOON, jd, observer)

    def sign_change_lookahead(self):
        return 2.7

    def aspect_lookahead(self):
        return 40 

    def mean_orbital_period(self):
        # http://hpiers.obspm.fr/eop-pc/models/constants.html
        return 27.32166155

    def average_motion_per_year(self):
        return 360 * 12 + 120

    def max_speed(self):
        return 15.3882655

    def speed_ratio(self, jd=None):
        # 11.76/d to 15.33deg/d
        if jd is None: jd = self.jd
        return (self.speed(jd) - 11.76) / 3.57

    def diameter_ratio(self, jd=None):
        # 29.3' to 34.1'
        if jd is None: jd = self.jd
        return (self.diameter(jd) - 29.3) / 4.8

    def dignity(self, jd=None):
        """Return the dignity of the planet at jd, or None."""
        if jd is None: jd = self.jd
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
        if jd is None: jd = self.jd
        return jd - self.last_new_moon().jd

    def period_length(self, jd=None):
        if jd is None: jd = self.jd
        return self.next_new_moon().jd - self.last_new_moon().jd

    def phase(self, jd=None):
        if jd is None: jd = self.jd
        sun = Sun()
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
        if jd is None: jd = self.jd
        sun = Sun()
        next_angle_jd, delta_jd, angle_diff = self.next_angle_to_planet(sun, 0, jd)
        return PlanetEvent('New moon in ' + self.sign(next_angle_jd), next_angle_jd)

    def last_new_moon(self, jd=None):
        if jd is None: jd = self.jd
        sun = Sun()
        next_angle_jd, delta_jd, angle_diff = self.next_angle_to_planet(sun, 0, jd, lookahead=-40)
        return PlanetEvent('New moon in ' + self.sign(next_angle_jd), next_angle_jd)

    def next_full_moon(self, jd=None):
        if jd is None: jd = self.jd
        sun = Sun()
        next_angle_jd, delta_jd, angle_diff = self.next_angle_to_planet(sun, 180, jd)
        return PlanetEvent('Full moon in ' + self.sign(next_angle_jd), next_angle_jd)

    def last_full_moon(self, jd=None):
        if jd is None: jd = self.jd
        sun = Sun()
        next_angle_jd, delta_jd, angle_diff = self.next_angle_to_planet(sun, 180, jd, lookahead=-40)
        return PlanetEvent('Full moon in ' + self.sign(next_angle_jd), next_angle_jd)

    def next_new_or_full_moon(self, jd=None):
        # TODO optimize
        next_new_moon = self.next_new_moon(jd)
        next_full_moon = self.next_full_moon(jd)
        if next_new_moon.jd < next_full_moon.jd:
            return next_new_moon
        else:
            return next_full_moon

    def last_new_or_full_moon(self, jd=None):
        # TODO optimize
        last_new_moon = self.last_new_moon(jd)
        last_full_moon = self.last_full_moon(jd)
        if last_new_moon.jd > last_full_moon.jd:
            return last_new_moon
        else:
            return last_full_moon

    def is_void_of_course(self, jd=None):
        """Whether the moon is void of course at a certain point in time.
        Returns a tuple (boolean, float) indicating whether it is void
        of course and up to which point in time."""
        # as per http://www.astrologyweekly.com/astrology-articles/void-of-course-moon.php
        # and http://www.estelledaniels.com/articles/VoidMoon.html
        # the traditional planets plus the major new ones (uranus, neptune, pluto) are used
        # plus the traditional aspects of conjunction, sextile, square, trine, opposition
        # another link:
        # http://www.lunarliving.org/moon/void-of-course-moon.html
        raise NotImplementedError
        if jd is None: jd = self.jd
        return (False, jd) # TODO

    def lunation_number(self):
        # TODO http://en.wikipedia.org/wiki/Lunation_Number
        raise NotImplementedError
        if jd is None: jd = self.jd
        return 0

class Mercury(Planet):
    def __init__(self, jd=None, observer=None):
        if jd is None: jd = jd_now()
        super(Mercury, self).__init__(sweph.MERCURY, jd, observer)

    def max_speed(self):
        return 2.2026512

    def sign_change_lookahead(self):
        # cf. http://www.keen.com/CommunityServer/UserBlogPosts/MaryAnneT/THE-PLANETS-IN-ORDER-OF-SPEED/513406.aspx
        return 75

    def aspect_lookahead(self):
        return 365 * 2.5 

    def aspect_possible(self, planet, angle):
        if planet.name() == 'Sun':
            return angle < 27.8 or angle > (360 - 27.8)
        if planet.name() == 'Venus':
            return angle < (27.8 + 47.8) or angle > (360 - (27.8 + 47.8))
        return True

    def dignity(self, jd=None):
        """Return the dignity of the planet at jd, or None."""
        if jd is None: jd = self.jd
        sign = self.sign(jd)
        if sign == 'Gemini':
            return 'rulership'
        elif sign == 'Virgo':
            return 'rulership/exaltation'
        elif sign == 'Sagittarius':
            return 'fall'
        elif sign == 'Pisces':
            return 'fall/detriment'
        else:
            return None

    def mean_orbital_period(self):
        return 87.9691

    def average_motion_per_year(self):
        return 360

class Venus(Planet):
    def __init__(self, jd=None, observer=None):
        if jd is None: jd = jd_now()
        super(Venus, self).__init__(sweph.VENUS, jd, observer)

    def max_speed(self):
        return 1.2598435

    def dignity(self, jd=None):
        """Return the dignity of the planet at jd, or None."""
        if jd is None: jd = self.jd
        sign = self.sign(jd)
        if sign == 'Libra':
            return 'rulership'
        if sign == 'Taurus':
            return 'rulership'
        elif sign == 'Pisces':
            return 'exaltation'
        elif sign == 'Virgo':
            return 'detriment'
        elif sign == 'Aries':
            return 'fall'
        elif sign == 'Scorpio':
            return 'fall'
        else:
            return None

    def sign_change_lookahead(self):
        return 150

    def aspect_lookahead(self):
        return 365 * 3.5 # roughly max time to conjunction/opposition with Mars

    def aspect_possible(self, planet, angle):
        if planet.name() == 'Sun':
            return angle < 47.8 or angle > (360 - 47.8)
        if planet.name() == 'Mercury':
            return angle < (27.8 + 47.8) or angle > (360 - (27.8 + 47.8))
        return True

    def average_motion_per_year(self):
        return 360


class Mars(Planet):
    def __init__(self, jd=None, observer=None):
        if jd is None: jd = jd_now()
        super(Mars, self).__init__(sweph.MARS, jd, observer)

    def max_speed(self):
        return 0.7913920

    def dignity(self, jd=None):
        """Return the dignity of the planet at jd, or None."""
        if jd is None: jd = self.jd
        sign = self.sign(jd)
        if sign == 'Aries':
            return 'rulership'
        if sign == 'Scorpio':
            return 'rulership'
        elif sign == 'Capricorn':
            return 'exaltation'
        elif sign == 'Cancer':
            return 'detriment'
        elif sign == 'Libra':
            return 'fall'
        elif sign == 'Taurus':
            return 'fall'
        else:
            return None

    def sign_change_lookahead(self):
        # Hopefully enough. FIXME: What is the maximum time for Mars
        # to be in a sign when exhibiting retrograde motion?
        # Its Rx in Libra in 2014 had it stay about 8 months, December to
        # and including July.
        return 30 * 10

    def aspect_lookahead(self):
        return 365 * 3.5 # roughly max time to conjunction/opposition with Jupiter

    def average_motion_per_year(self):
        return 180


class Jupiter(Planet):
    def __init__(self, jd=None, observer=None):
        if jd is None: jd = jd_now()
        super(Jupiter, self).__init__(sweph.JUPITER, jd, observer)

    def max_speed(self):
        return 0.2423810

    def dignity(self, jd=None):
        """Return the dignity of the planet at jd, or None."""
        if jd is None: jd = self.jd
        sign = self.sign(jd)
        if sign == 'Sagittarius':
            return 'rulership'
        if sign == 'Pisces':
            return 'rulership'
        elif sign == 'Cancer':
            return 'exaltation'
        elif sign == 'Capricorn':
            return 'detriment'
        elif sign == 'Gemini':
            return 'fall'
        elif sign == 'Virgo':
            return 'fall'
        else:
            return None

    def sign_change_lookahead(self):
        return 365 * 1.5 # should be ample enough.

    def aspect_lookahead(self):
        # https://en.wikipedia.org/wiki/Great_conjunction#Great_Conjunctions_in_ecliptical_longitude_between_1800_and_2100
        return 365 * 23 # roughly max time to conjunction/opposition with Saturn

    def average_motion_per_year(self):
        return 30

class Saturn(Planet):
    def __init__(self, jd=None, observer=None):
        if jd is None: jd = jd_now()
        super(Saturn, self).__init__(sweph.SATURN, jd, observer)

    def max_speed(self):
        return 0.1308402

    def dignity(self, jd=None):
        """Return the dignity of the planet at jd, or None."""
        if jd is None: jd = self.jd
        sign = self.sign(jd)
        if sign == 'Capricorn':
            return 'rulership'
        if sign == 'Aquarius':
            return 'rulership'
        elif sign == 'Libra':
            return 'exaltation'
        elif sign == 'Aries':
            return 'detriment'
        elif sign == 'Cancer':
            return 'fall'
        elif sign == 'Leo':
            return 'fall'
        else:
            return None

    def sign_change_lookahead(self):
        return 365 * 3.5

    def aspect_lookahead(self):
        return 365 * 30 + 365 * 40 # to Chiron

    def average_motion_per_year(self):
        return 12

def days_frac_to_dhms(days_frac):
    """Convert a day float to integer days, hours, minutes and seconds.

    Returns a tuple (days, hours, minutes, seconds).
    
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

# TODO use astropy.coordinates.EarthLocation instead, when it's
# available (v0.4)
class LatLong():
    def __init__(self, lat, long):
        if lat > 90 or lat < -90:
            raise ValueError("Latitude must be between -90 and 90")
        if long > 180 or long < -180:
            raise ValueError("Longitude must be between -180 and 180")
        self.lat = lat
        self.long = long

def compute_sun_data(jd=None, observer=None):
    """Collect data for the sun.

     :param jd: reference date as Julian day, defaults to :func:`jd_now`
     :type jd: float or None
     :param observer: pass the observer position to have the output
                      include rise and set times.
     :type observer: LatLong or None
     :returns: a collection of sun data
     :rtype: OrderedDict
     """
    if jd is None: jd = jd_now()

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
    """Collect data for the moon.

     :param jd: reference date as Julian day, defaults to :func:`jd_now`
     :type jd: float or None
     :param observer: pass the observer position to have the output
                      include rise and set times.
     :type observer: LatLong or None
     :returns: a collection of sun data
     :rtype: OrderedDict
     """
    if jd is None: jd = jd_now()

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

def get_events(jd_start, jd_end, limit=100, type='%', subtype='%', planet='%', data='%'):
    # TODO we only support AND of filters, not OR
    import sqlite3

    conn = sqlite3.connect(dbfile)

    conn.row_factory = sqlite3.Row

    c = conn.cursor()

    sql = """SELECT * FROM events
             WHERE jd BETWEEN ? AND ?
               AND type LIKE ? AND subtype LIKE ?
               AND planet LIKE ? AND data LIKE ?
             ORDER BY jd ASC
             LIMIT ?"""
    rows = c.execute(sql, (jd_start, jd_end, type, subtype, planet, data, limit))

    result = []
    for row in rows:
        dict = collections.OrderedDict()
        for key in ['jd', 'type', 'subtype', 'planet', 'data']:
                dict[key] = row[key]
        dict['iso_date'] = jd2iso(row['jd'])
        dict['delta_days'] = row['jd'] - jd_start
        result.append(dict)

    return result


def generate_event_table(jd_start, jd_end):
    # TODO this generates events that slightly exceed jd_end
    import sqlite3
    
    conn = sqlite3.connect(dbfile)

    c = conn.cursor()

    c.execute('DROP TABLE events')
    c.execute('CREATE TABLE IF NOT EXISTS events (jd float, type text, subtype text, planet text, data text)')
    c.execute('DELETE FROM events')

    def pump_events(event_function):
        flush_counter = 0
        jd = jd_start
        while jd < jd_end:
            event = event_function(jd)
            if event is None:
                # see comment on "Mercury sextile Venus' below.
                jd += 365 * 2.4
                continue

            event_jd, event_type, event_subtype, event_planet, event_data = event_function(jd)
            assert(event_jd >= jd)
            assert(event_type)
            assert(event_planet)

            if event_subtype is None:
                event_subtype = ''
            if event_data is None:
                event_data = ''


            percentage = (jd - jd_start) / (jd_end - jd_start) * 100
            print('%f%%' % percentage, event_jd, jd2iso(event_jd), event_type,
                    event_subtype, event_planet, event_data)

            c.execute("INSERT INTO events VALUES (?, ?, ?, ?, ?)", 
                    (event_jd, event_type, event_subtype, event_planet, event_data))

            # 1 day is reasonable for the smallest event we handle (Moon ingress)
            jd = event_jd + 1

            flush_counter += 1
            if flush_counter % 100 == 0:
                conn.commit()

    # types: "square dexter" ... conjunction ... ingress retrograde direct
    # type / p1 / p2 or sign or NULL
    # frontend: "Mercury Rx in Pisces square dexter Saturn in Sagittarius"
    # db: "Mercury square dexter Saturn"

    # TODO retrograde/direct events, more planets

    # aspects
    planets = [Moon(), Sun(), Mercury(), Venus(), Mars(), Jupiter(), Saturn()]
    #aspects = [(72, 'quintile', 'dexter'), (288, 'quintile', 'sinister')]
    #planets = [Venus(), Mars()]
    #aspects = [(30, 'sextile', 'dexter')]
    for planet in planets:
        for partner_planet in planets:
            if partner_planet.max_speed() < planet.max_speed():
                for aspect in aspects:
                    aspect_angle, aspect_name, aspect_mode = aspect
                    if planet.aspect_possible(partner_planet, aspect_angle):
                        event_type = aspect_name
                        event_subtype = aspect_mode
                        def event_function(jd):
                            next_angle = planet.next_angle_to_planet(partner_planet, aspect_angle, jd)
                            if next_angle:
                                event_jd, delta_days, angle_diff = next_angle
                                return (event_jd, event_type, event_subtype, planet.name(), partner_planet.name())
                            # Mercury sextile Venus is possible but might not happen in the standard lookahead
                            # period. Mercury quintile Venus is even unlikelier. In both cases we want to skip
                            # the current lookahead period and look in the next.
                            assert(planet.name() in ['Mercury', 'Venus'] and
                                   partner_planet.name() in ['Mercury', 'Venus'] and
                                   aspect_angle >= 60)
                            print('Note: no %s (%s) aspect between %s and %s in period starting %f.' %
                                    (aspect_name, aspect_mode, planet.name(), partner_planet.name(), jd))
                            return None
                        pump_events(event_function)

    # ingresses
    for planet in planets:
        def event_function(jd):
            event_jd = planet.next_sign_change(jd)
            event_type = 'ingress'
            event_subtype = None
            event_planet = planet.name()
            event_data = planet.sign(event_jd)
            return (event_jd, event_type, event_subtype, event_planet, event_data)
        pump_events(event_function)

    # retrogrades
    # TODO
    #for planet in [Mercury(), Venus(), Mars(), Jupiter(), Saturn()]:
    #    def event_function(jd):
    #        event_jd = planet.next_rx_event(jd)
    #        event_description = '%s turns retrograde' % (planet.name(), planet.sign(event_jd))
    #        return (event_jd, event_description)
    #        
    #    pump_events(event_function)

    conn.commit()

    conn.close()

def print_moon_events():
    import sqlite3
    conn = sqlite3.connect('moon-events.db')

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

    if debug_event_approximation:
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
# * upcoming event stream:
#    https://play.google.com/store/apps/details?id=uk.co.lunarium.iluna
#    http://www.lunarliving.org/

# http://starchild.gsfc.nasa.gov/docs/StarChild/questions/question5.html

# events to subscribe to:
# full, new, 1st quarter, 3rd quarter, sign change, void of course, aspect (one of subset X) to planet (one of subset Y)

# LATER:
# use astropy.time.Time everywhere
# use astropy.coordinates.EarthLocation (astropy 0.4)
#
# merge compute_*_data functions into one
#
# lunar standstills
# moon out of sun's declination band

def compute_min_max_speeds():
    for p in [Moon(), Sun(), Mercury(), Venus(), Mars(), Jupiter(), Saturn()]:
        min = 1000
        max = 0
        jd = jd_now()
        while jd < jd_now()+365*100:
            if p.speed(jd) > max:
                max = p.speed(jd)
            if p.speed(jd) < min:
                min = p.speed(jd)
            jd += 1
        print(p, min, max)

# TODO: move Planet stuff to separate file planets.py
