#!/bin/sh
''''exec nosetests -s -- "$0" ${1+"$@"} # '''

import numpy as np
from cerridwen import *

np.set_printoptions(threshold=1000)

print(jd_now())

jd_start = iso2jd('2014-10-01 7:40:00Z')

rx_periods = Mercury(jd_start).retrogrades_within_period(jd_start, jd_start+30)
print(rx_periods)
print(list(map(lambda x: jd2iso(x['jd']), rx_periods)))

print(jd2iso(Mercury(jd_start).next_rx_event()[0]))

#jd_start = iso2jd('0900-07-01 7:40:00Z')
#jd_end = jd_start + 365*1300
#generate_event_table(jd_start, jd_end, [Jupiter(), Saturn()],
#        [(0,'conjunction',None)], compute_ingresses=False)

#for event in get_events(jd_now(), jd_now()+400, planet='mercury', type='ingress'): print(event, '\n')

