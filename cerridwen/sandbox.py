#!/bin/sh
''''exec nosetests -s -- "$0" ${1+"$@"} # '''

import numpy as np
from .utils import *
from .planets import *
from .event_generator import *

def sandbox_test():
    np.set_printoptions(threshold=1000)

    print(jd_now())

    jd_start = iso2jd('2014-10-01 7:40:00')

    rx_periods = Mercury(jd_start).retrogrades_within_period(jd_start, jd_start+30)
    print(rx_periods)
    print(list(map(lambda x: jd2iso(x['jd']), rx_periods)))

    print(jd2iso(Mercury(jd_start).next_rx_event()['jd']))

    jd_start = jd_now()
    jd_end = jd_start + 365*0.5
    #generate_event_table(jd_start, jd_end, [Jupiter(), Saturn()], [(0,'conjunction',None)], compute_ingresses=False)
    generate_event_table(jd_start, jd_end)

    #for event in get_events(jd_now(), jd_now()+400, planet='mercury', type='retrograde'): print(event, '\n')


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

