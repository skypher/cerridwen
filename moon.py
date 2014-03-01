#!/usr/bin/python3

# license: GPL3

# terminology note: "planet" is used in the astrological sense, i.e.
# also for the sun, moon and asteroids.

import swisseph as sweph
import time
import math
import numpy as np

sweph.set_ephe_path('/home/sky/eph/sweph')

def jd_now():
    gmtime = time.gmtime()
    return sweph.julday(gmtime.tm_year,
                        gmtime.tm_mon,
                        gmtime.tm_mday,
                        gmtime.tm_hour+((gmtime.tm_min * 100 / 60) / 100))

signs = ['Ari','Tau','Gem','Can','Leo','Vir',
         'Lib','Sco','Sag','Cap','Aqu','Pis'];

class Planet:
    def __init__(self, planet_id):
        self.id = planet_id

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
        return (sign, reldeg, minutes, long)

    def speed(self, jd=jd_now()):
        speed = sweph.calc_ut(jd, self.id)[3]
        return speed

    def is_rx(self, jd=jd_now()):
        speed = self.speed(jd)
        return speed < 0

    def is_stationing(self, jd=jd_now()):
        speed = self.speed(jd)
        return math.fabs(speed) < 0.2

    def angle(self, planet, jd=jd_now()):
        return (self.longitude(jd) - planet.longitude(jd)) % 360

    def illumination(self, jd=jd_now()):
        sun = Planet(sweph.SUN)
        return self.angle(sun, jd) / 360.0

    def next_angle_to_planet(self, planet, target_angle, jd=jd_now(), orb="auto", lookahead="auto"):
        # TODO: set lookahead, sampling_interval and orb according to the speed of planets involved.
        # TODO: honor orb
        assert(target_angle<360)
        if lookahead == "auto":
            lookahead = 80 # days
        next_angles = self.angles_to_planet_within_period(planet,
                target_angle, jd, jd+lookahead)
        if next_angles:
            next_angle_jd = next_angles[0]
        delta_jd = next_angle_jd - jd
        return (next_angle_jd, delta_jd)

    def angles_to_planet_within_period(self, planet, target_angle, jd_start, jd_end, sample_interval="auto", passes=3):
        # TODO: take a closer look at the interesting areas and sample again
        # with high freq to get more accurate results.
        assert(target_angle<360)
        if sample_interval == "auto":
            sample_interval = 1/4 # days
        #print('atpwp: start=%f, end=%f, interval=%f, sample_pass=%d' % (jd_start, jd_end, sample_interval, passes))
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

        matching_angles = angle_at_jd_v(matching_jds)
        print(matching_jds, matching_angles)

        def pair_means(a):
            odds = a[::2]
            evens = a[1::2]
            return np.vectorize(lambda x,y: (x+y)/2)(odds,evens)
        jd_means = pair_means(matching_jds)
        angle_means = pair_means(matching_angles)
        #print(jd_means, angle_means)

        if passes:
            # FIXME; need more than just the first pair
            result = self.angles_to_planet_within_period(planet, target_angle, matching_jds[0],
                    matching_jds[1], sample_interval*(1/30), passes-1)
            if result is None:
                return jd_means.tolist()
            else:
                return result
        else:
            return jd_means.tolist()


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
            phase = "waxing crescent"
        elif 90 <= angle < 180:
            phase = "waxing gibbous"
        elif 190 <= angle < 270:
            phase = "waning gibbous"
        else:
            phase = "waning crescent"

        return (phase, quarter, quarter_english)

    def next_new_moon(self, jd=jd_now()):
        sun = Planet(sweph.SUN)
        return self.next_angle_to_planet(sun, 0, jd)

    def next_full_moon(self, jd=jd_now()):
        sun = Planet(sweph.SUN)
        return self.next_angle_to_planet(sun, 180, jd)

def format_jd(jd):
    year, month, day, hour_frac = sweph.revjul(jd)
    hours = math.floor(hour_frac)
    minutes = (hour_frac % 1) * 60
    return "%d-%d-%d %d:%d" % (year, month, day, hours, minutes)

def days_frac_to_dhm(days_frac):
    """Convert a day float to integer days, hours and minutes.

    Returns a tuple (days, hours, minutes).
    
    >>> days_frac_to_dhm(2.53)
    (2, 12, 43)
    """
    days = math.floor(days_frac)
    hours_minutes_frac = days_frac - days
    hours = math.floor(hours_minutes_frac * 24)
    minutes_frac = hours_minutes_frac - hours / 24
    minutes = math.floor(minutes_frac * 1440)

    return (days, hours, minutes)


if __name__ == '__main__':
    import doctest
    doctest.testmod()

    print(time.asctime())
    print(format_jd(jd_now()))

    moon = Moon()
    sign, deg, minutes, long = moon.position()
    print('%s: %.2f %d %s %d\'' % (moon.name(), long, deg, sign, minutes))

    sun = Planet(sweph.SUN)
    sign, deg, minutes, long = sun.position()
    print('%s: %.2f %d %s %d\'' % (sun.name(), long, deg, sign, minutes))

    phase, quarter, quarter_english = moon.phase()
    print("phase: %s, quarter: %s, illum: %d%%" % (phase, quarter_english, moon.illumination() * 100))
    next_new_moon_jd, next_new_moon_jd_delta = moon.next_new_moon()
    print("next new moon: in %d days XX hours (%s)" % (next_new_moon_jd_delta, format_jd(next_new_moon_jd)))
    next_full_moon_jd, next_full_moon_jd_delta = moon.next_full_moon()
    print("next full moon: in %d days XX hours (%s)" % (next_full_moon_jd_delta, format_jd(next_full_moon_jd)))

# sign degrees minutes illumination waxing/waning gibbous/crescent next_new next_full last_new last_full size distance folk name
# age moon name distance diameter angle to sun

