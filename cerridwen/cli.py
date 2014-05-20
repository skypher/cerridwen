#!/usr/bin/python3

import cerridwen

import time

def emit_text(result):
    # TODO build string and return
    print('Julian day:', result['jd'])
    print('Universal time (UTC):', result['iso_date'])
    print('Local time:', time.asctime())

    sign, deg, minutes = result['position'].rel_tuple
    print('Moon: %d %s %d\'' % (deg, sign[:3], minutes))

    sign, deg, minutes = result['sun'].rel_tuple
    print('Sun: %d %s %d\'' % (deg, sign[:3], minutes))

    trend, shape, quarter, quarter_english = result['phase']
    phase = trend + ' ' + shape
    print("phase: %s, quarter: %s, illum: %d%%" %
            (phase, quarter_english, result['illumination'] * 100))

    next_new_moon = result['next_new_moon']
    print("next new moon: %s: in %s (%s / %f)" %
            (next_new_moon.description,
             cerridwen.render_delta_days(next_new_moon.delta_days),
             cerridwen.jd2iso(next_new_moon.jd), next_new_moon.jd))

    next_full_moon = result['next_full_moon']
    print("next full moon: %s: in %s (%s / %f)" %
            (next_full_moon.description,
             cerridwen.render_delta_days(next_full_moon.delta_days),
             cerridwen.jd2iso(next_full_moon.jd), next_full_moon.jd))

def main():
    import argparse
    parser = argparse.ArgumentParser()
    args = parser.parse_args()

    cerridwen.quicktest()

    emit_text(cerridwen.compute_moon_data());

if __name__ == '__main__':
    main()

