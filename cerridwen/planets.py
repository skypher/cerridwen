# terminology note: "planet" is used in the astrological sense, i.e.
# also for the sun, moon and asteroids. we also sometimes use
# "planet" when the point in question is something like the AC.

import collections
import math
import numpy as np
import swisseph as sweph

from .defs import *
from .utils import *
from .approximate import approximate_event_date

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
        jd = sweph.rise_trans(self.jd, self.id, self.observer.long, self.observer.lat, rsmi=sweph.CALC_RISE)[1][0]
        return PlanetEvent('%s rises' % self.name(), jd)

    def next_set(self):
        if self.observer is None:
            raise ValueError('Rise/set times require observer longitude and latitude')
        jd = sweph.rise_trans(self.jd, self.id, self.observer.long, self.observer.lat, rsmi=sweph.CALC_SET)[1][0]
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

            # [NOTE1] each diff pass results in an array that is one element smaller.
            # newer versions of numpy actually check for the boolean array size
            # to match the base array's size, so we have to fudge it.
            is_minimum = np.append(is_minimum, [False, False])
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

            # account for np.diff, see [NOTE1]
            is_zero_crossing = np.append(is_zero_crossing, [False])
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

class Uranus(Planet):
    def __init__(self, jd=None, observer=None):
        if jd is None: jd = jd_now()
        super(Uranus, self).__init__(sweph.URANUS, jd, observer)

class Neptune(Planet):
    def __init__(self, jd=None, observer=None):
        if jd is None: jd = jd_now()
        super(Neptune, self).__init__(sweph.NEPTUNE, jd, observer)

class Pluto(Planet):
    def __init__(self, jd=None, observer=None):
        if jd is None: jd = jd_now()
        super(Pluto, self).__init__(sweph.PLUTO, jd, observer)

