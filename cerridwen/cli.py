#!/usr/bin/python3

import cerridwen

import time

def emit_time_info(result):
    print('Julian day:', result['jd'])
    print('Universal time (UTC):', result['iso_date'])
    print('Local time:', time.asctime())

def emit_sun_text(result):
    sign, deg, min, sec = result['position'].rel_tuple
    print('Sun: %f / %d %s %d\' %d"' % (result['position'].absolute_degrees,
        deg, sign[:3], min, sec))

def emit_moon_text(result):
    sign, deg, min, sec = result['position'].rel_tuple
    print('Moon: %f / %d %s %d\' %d"' % (result['position'].absolute_degrees,
        deg, sign[:3], min, sec))


    trend, shape, quarter, quarter_english = result['phase']
    phase = trend + ' ' + shape
    print("phase: %s, quarter: %s, illum: %d%%" %
            (phase, quarter_english, result['illumination'] * 100))

    next_new_moon = result['next_new_moon']
    print("next new moon: %s: in %s (%s / %f)" %
            (next_new_moon.description,
             cerridwen.utils.render_delta_days(next_new_moon.delta_days),
             cerridwen.jd2iso(next_new_moon.jd), next_new_moon.jd))

    next_full_moon = result['next_full_moon']
    print("next full moon: %s: in %s (%s / %f)" %
            (next_full_moon.description,
             cerridwen.utils.render_delta_days(next_full_moon.delta_days),
             cerridwen.jd2iso(next_full_moon.jd), next_full_moon.jd))

def main():
    import argparse
    parser = argparse.ArgumentParser(prog='Cerridwen')
    parser.add_argument('--version', action='version', version='%(prog)s ' + cerridwen.__VERSION__)
    args = parser.parse_args()

    #cerridwen.quicktest()

    sun_data = cerridwen.compute_sun_data()
    emit_time_info(sun_data)
    emit_sun_text(sun_data)
    emit_moon_text(cerridwen.compute_moon_data())

if __name__ == '__main__':
    main()

