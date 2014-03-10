#!/usr/bin/python3

# license: GPL3

# terminology note: "planet" is used in the astrological sense, i.e.
# also for the sun, moon and asteroids.

debug_angle_finder = 0

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
    def __init__(self, planet_id):
        self.id = planet_id
        self.Position = collections.namedtuple('Position',
                ['sign', 'degrees', 'minutes', 'absolute_degrees'])
        self.AngleTime = collections.namedtuple('AngleTime',
                ['jd', 'delta_jd', 'angle_diff'])

    def name(self):
        return sweph.get_planet_name(self.id)

    def diameter(self, jd=jd_now()):
        """The apparent diameter of the planet, in arc minutes."""
        return sweph.pheno_ut(jd, self.id)[3] * 60

    def longitude(self, jd=jd_now()):
        long = sweph.calc_ut(jd, self.id)[0]
        return long

    def distance(self, jd=jd_now()):
        distance = sweph.calc_ut(jd, self.id)[2]
        return distance

    def position(self, jd=jd_now()):
        long = sweph.calc_ut(jd, self.id)[0]
        sign = signs[int(long / 30)]
        reldeg = long % 30.0
        minutes = ((reldeg % 1) * 100) * 60 / 100
        return self.Position._make([sign, reldeg, minutes, long])

    def sign(self, jd=jd_now()):
        return self.position(jd)[0]

    def speed(self, jd=jd_now()):
        speed = sweph.calc_ut(jd, self.id)[3]
        return speed

    def is_rx(self, jd=jd_now()):
        speed = self.speed(jd)
        return speed < 0

    def is_stationing(self):
        # http://houseofdaedalus.blogspot.de/2012/07/meaning-of-retrograde-motion.html
        speed = self.speed()
        return math.fabs(speed) < 0.2

    def angle(self, planet, jd=jd_now()):
        return (self.longitude(jd) - planet.longitude(jd)) % 360

    def illumination(self, jd=jd_now()):
        sun = Planet(sweph.SUN)
        return self.angle(sun, jd) / 360.0

    def next_angle_to_planet(self, planet, target_angle, jd=jd_now(),
                             orb="auto", lookahead="auto"):
        """Return (jd, delta_jd) indicating the time of the next target_angle
        to a planet.
        Return None if no result could be found in the requested lookahead
        period."""
        # TODO: set lookahead, sampling_interval and orb according to the speed
        #       of planets involved, if "auto".
        # TODO: honor orb
        assert(target_angle<360)
        if lookahead == "auto":
            lookahead = 34 # days
        next_angles = self.angles_to_planet_within_period(planet, target_angle, jd, jd+lookahead)
        if next_angles:
            next_angle_jd = next_angles[0]['jd']
            delta_jd = next_angle_jd - jd
            angle_diff = mod360_fabs(target_angle, next_angles[0]['angle'])
            assert(angle_diff < 0.0001)
            return (next_angle_jd, delta_jd, angle_diff)
        else:
            return None

    def angles_to_planet_within_period(self, planet, target_angle, jd_start, jd_end, sample_interval="auto", passes=6):
        assert(target_angle<360)
        if sample_interval == "auto":
            sample_interval = 1/20 # days
        if debug_angle_finder:
            print('atpwp (:=%f deg): start=%f (%s), end=%f (%s), interval=%f, sample_pass=%d'
                    % (target_angle, jd_start, format_jd(jd_start), jd_end, format_jd(jd_end),
                        sample_interval, passes))
        jds = np.arange(jd_start, jd_end, sample_interval)
        def angle_at_jd(d):
            return self.angle(planet, d)
        angle_at_jd_v = np.vectorize(angle_at_jd)
        angles = angle_at_jd_v(jds)
        target_adjusted_angles = (angles - target_angle) % 360
        sign_changes = np.roll(np.diff(np.sign(np.diff(target_adjusted_angles))) != 0, 1)
        matching_jds = jds[sign_changes]

        if matching_jds.size < 2:
            return None

        matches = []
        jd_starts = matching_jds[::2]
        jd_ends = matching_jds[1::2]
        for i in range(jd_starts.size):
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
            return {'jd': jd_mean, 'angle': angle_at_jd(jd_mean)}

        refined_matches = []
        if passes:
            for match in matches:
                result = self.angles_to_planet_within_period(planet,
                        target_angle, match['jd_start'], match['jd_end'],
                        sample_interval*(1/100), passes-1)
                if result:
                    refined_matches += result
                else:
                    if debug_angle_finder:
                        print('Notice: stopping angle finder with %d passes remaining.' % passes)
                    refined_matches.append(match_mean(match))
        else:
            for match in matches:
                refined_matches.append(match_mean(match))

        return refined_matches

    def next_sign_change(self, jd=jd_now()):
        # TODO
        return jd

    def time_left_in_sign(self, jd=jd_now()):
        # TODO
        return jd

class Sun(Planet):
    def __init__(self):
        super(Sun, self).__init__(sweph.SUN)

class Moon(Planet):
    def __init__(self):
        super(Moon, self).__init__(sweph.MOON)

    def diameter_ratio(self, jd=jd_now()):
        return (self.diameter(jd) - 29.3) / 4.8

    def dignity(self, jd=jd_now()):
        sign = self.sign(jd)
        if sign == 'Cancer':
            return 'domicile'
        elif sign == 'Taurus':
            return 'exaltation'
        elif sign == 'Capricorn':
            return 'detriment'
        elif sign == 'Scorpio':
            return 'fall'
        else:
            return None

    def phase(self, jd=jd_now()):
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
            quarter_english = ["new", "first quarter", "full", "third quarter"][quarter]

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

    def next_new_moon(self, jd=jd_now()):
        """
        >>> math.floor(Moon().next_new_moon(2456720.24305)[0])
        2456747
        """
        sun = Planet(sweph.SUN)
        next_angle_jd, delta_jd, angle_diff = self.next_angle_to_planet(sun, 0, jd)
        return self.AngleTime._make([next_angle_jd, delta_jd, angle_diff])

    def next_full_moon(self, jd=jd_now(), as_dict=False):
        sun = Planet(sweph.SUN)
        next_angle_jd, delta_jd, angle_diff = self.next_angle_to_planet(sun, 180, jd)
        return self.AngleTime._make([next_angle_jd, delta_jd, angle_diff])

    def is_void_of_course(self, jd=jd_now()):
        """Whether the moon is void of course at a certain point in time.
        Returns a tuple (boolean, float) indicating whether it is void
        of course and up to which point in time."""
        raise NotImplementedError
        return (False, jd) # TODO

    def lunation_number(self):
        # TODO http://en.wikipedia.org/wiki/Lunation_Number
        raise NotImplementedError
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

if __name__ == '__main__':
    if debug_angle_finder:
        moon = Moon()
        print(format_jd(moon.next_full_moon()[0]))
        print(format_jd(moon.next_new_moon()[0]))
        sys.exit(1)

    import doctest
    doctest.testmod()

    result = collections.OrderedDict()

    result['jd'] = jd_now()
    result['utc'] = format_jd(jd_now())

    moon = Moon()
    result['moon'] = moon.position()

    sun = Sun()
    result['sun'] = sun.position()

    result['phase'] = moon.phase()
    result['illumination'] = moon.illumination()
    result['distance'] = moon.distance()
    result['diameter'] = moon.diameter()
    result['diameter_ratio'] = moon.diameter_ratio()
    result['next_new_moon'] = moon.next_new_moon()
    result['next_full_moon'] = moon.next_full_moon()

    result['dignity'] = moon.dignity()

    def emit_text(result):
        print('Julian day:', result['jd'])
        print('Universal time (UTC):', result['utc'])
        print('Local time:', time.asctime())

        sign, deg, minutes = result['moon'][:3]
        print('%s: %d %s %d\'' % (moon.name(), deg, sign[:3], minutes))

        sign, deg, minutes = result['sun'][:3]
        print('%s: %d %s %d\'' % (sun.name(), deg, sign[:3], minutes))

        trend, shape, quarter, quarter_english = result['phase']
        phase = trend + ' ' + shape
        print("phase: %s, quarter: %s, illum: %d%%" %
                (phase, quarter_english, result['illumination'] * 100))

        next_new_moon_jd, next_new_moon_jd_delta, angle_diff = result['next_new_moon']
        days, hours = days_frac_to_dhms(next_new_moon_jd_delta)[:2]
        print("next new moon: in %d days %d hours (%s), %fdeg exact" %
                (days, hours, format_jd(next_new_moon_jd), angle_diff))

        next_full_moon_jd, next_full_moon_jd_delta, angle_diff = result['next_full_moon']
        days, hours = days_frac_to_dhms(next_full_moon_jd_delta)[:2]
        print("next full moon: in %d days %d hours (%s), %fdeg exact" %
                (days, hours, format_jd(next_full_moon_jd), angle_diff))

    def emit_json(result):
        # Note: simplejson treats namedtuples as dicts by default but this is
        # one dep less.
        for field in ['moon', 'sun', 'phase', 'next_new_moon', 'next_full_moon']:
            result[field] = result[field]._asdict()
        result['next_new_moon']['utc'] = format_jd(result['next_new_moon']['jd'])
        result['next_full_moon']['utc'] = format_jd(result['next_full_moon']['jd'])
        import json
        print(json.dumps(result, indent=8))

    emit_text(result);
    emit_json(result);


# VERSION 1: diameter, age
# LATER: last_new last_full folk name period_length lunation_number

