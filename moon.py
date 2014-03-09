#!/usr/bin/python3

# license: GPL3

# terminology note: "planet" is used in the astrological sense, i.e.
# also for the sun, moon and asteroids.

import swisseph as sweph
import time, calendar
import math
import numpy as np
import collections

sweph.set_ephe_path('/home/sky/eph/sweph')

def jd_now():
    gmtime = time.gmtime()
    return sweph.julday(gmtime.tm_year,
                        gmtime.tm_mon,
                        gmtime.tm_mday,
                        gmtime.tm_hour+((gmtime.tm_min * 100 / 60) / 100))

signs = ['Aries','Taurus','Gemini','Cancer','Leo','Virgo',
         'Libra','Scorpio','Sagittarius','Capricorn','Aquarius','Pisces'];

traditional_major_aspects = [0, 60, 90, 120, 180, 270, 300]

class Planet:
    def __init__(self, planet_id):
        self.id = planet_id
        self.Position = collections.namedtuple('Position',
                ['sign', 'degrees', 'minutes', 'absolute_degrees'])
        self.Timespec = collections.namedtuple('Timespec',
                ['jd', 'delta_jd'])

    def name(self):
        return sweph.get_planet_name(self.id)

    def longitude(self, jd=jd_now()):
        long = sweph.calc_ut(jd, self.id)[0]
        return long

    def position(self, jd=jd_now()):
        long = sweph.calc_ut(jd, self.id)[0]
        sign = signs[int(long / 30)]
        reldeg = long % 30.0
        minutes = ((reldeg % 1) * 100) * 60 / 100
        return self.Position._make([sign, reldeg, minutes, long])

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
            lookahead = 80 # days
        next_angles = self.angles_to_planet_within_period(planet, target_angle, jd, jd+lookahead)
        if next_angles:
            next_angle_jd = next_angles[0]['jd']
            delta_jd = next_angle_jd - jd
            return (next_angle_jd, delta_jd)
        else:
            return None

    def angles_to_planet_within_period(self, planet, target_angle, jd_start, jd_end, sample_interval="auto", passes=6):
        assert(target_angle<360)
        if sample_interval == "auto":
            sample_interval = 1/4 # days
        if False:
            print('atpwp: start=%f, end=%f, interval=%f, sample_pass=%d'
                    % (jd_start, jd_end, sample_interval, passes))
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
            matches.append({'jd_start':jd_start, 'jd_end':jd_end,
                'angle_start': angle_at_jd(jd_start),
                'angle_end': angle_at_jd(jd_end)});

        def match_mean(match):
            jd_mean = (match['jd_start'] + match['jd_end']) / 2
            angle_mean = ((match['angle_start'] + match['angle_end']) % 360) / 2
            return {'jd': jd_mean, 'angle': angle_mean}

        refined_matches = []
        if passes:
            for match in matches:
                result = self.angles_to_planet_within_period(planet,
                        target_angle, match['jd_start'], match['jd_end'],
                        sample_interval*(1/1000), passes-1)
                if result:
                    refined_matches += result
                else:
                    #print('Notice: stopping angle finder with %d passes remaining.' % passes)
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
        next_angle_jd, delta_jd = self.next_angle_to_planet(sun, 0, jd)
        return self.Timespec._make([next_angle_jd, delta_jd])

    def next_full_moon(self, jd=jd_now(), as_dict=False):
        sun = Planet(sweph.SUN)
        next_angle_jd, delta_jd = self.next_angle_to_planet(sun, 180, jd)
        return self.Timespec._make([next_angle_jd, delta_jd])

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
    time_ = time.mktime((year,month,day,hours,minutes,seconds,0,0,0))
    return time.strftime('%Y-%m-%dT%H:%M:%SZ', time.localtime(time_))

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
    result['next_new_moon'] = moon.next_new_moon()
    result['next_full_moon'] = moon.next_full_moon()

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

        next_new_moon_jd, next_new_moon_jd_delta = result['next_new_moon']
        days, hours = days_frac_to_dhms(next_new_moon_jd_delta)[:2]
        print("next new moon: in %d days %d hours (%s)" %
                (days, hours, format_jd(next_new_moon_jd)))

        next_full_moon_jd, next_full_moon_jd_delta = result['next_full_moon']
        days, hours = days_frac_to_dhms(next_full_moon_jd_delta)[:2]
        print("next full moon: in %d days %d hours (%s)" %
                (days, hours, format_jd(next_full_moon_jd)))

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


# VERSION 1: status (exalted etc.), diameter, distance, age
# LATER: last_new last_full folk name period_length, lunation_number

