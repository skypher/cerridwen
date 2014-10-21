#!/usr/bin/python3

import math
import numpy as np

import cerridwen

from .defs import debug_event_approximation, maximum_error, max_data_points  

def approximate_event_date(jd_start, jd_end, match_finder, match_filter,
                           distance_function=math.fabs,
                           sample_interval=1/10, passes=8):
    # XXX debug help
    #passes = 0
    #sample_interval = 1/10

    num_data_points = abs(jd_end - jd_start) / sample_interval
    #print('data points', num_data_points)
    if num_data_points > max_data_points:
        # this used to be a safeguard against bogus matches in
        # certain Rx periods. It still makes sense to leave this
        # in for a while.
        #sample_interval = 1/1000
        #if debug_event_approximation:
        #    print('data point maximum (%d) exceeded (have %d), reducing sample interval to %f.' %
        #            (max_data_points, num_data_points, sample_interval))
        if debug_event_approximation:
            print('data point maximum (%d) exceeded (have %d), aborting pass.' %
                    (max_data_points, num_data_points))
        return None
    if debug_event_approximation:
        print('atpwp (:=%s deg): start=%f (%s), end=%f (%s), interval=%f, '
              'sample_pass=%d'
              % ('?', jd_start, cerridwen.jd2iso(jd_start), jd_end,
                 cerridwen.jd2iso(jd_end), sample_interval, passes))

    jds = np.arange(jd_start, jd_end, sample_interval)

    matches, data_fn = match_finder(jds); 

    if matches is None:
        return None

    refined_matches = dict()
    for jd, value in matches.items():
        fuzz = sample_interval * 2
        jd_fuzz_start = jd - fuzz 
        jd_fuzz_end = jd + fuzz
        distance = distance_function(data_fn(jd_fuzz_start), data_fn(jd_fuzz_end))
        precision_reached = (distance < maximum_error)
        if precision_reached and debug_event_approximation:
            print('STOP: precision reached (distance %10f, max %10f)' % (distance, maximum_error))
        if passes and not precision_reached:
            new_sample_interval = sample_interval * (1/100)
            extra_fuzz = fuzz + new_sample_interval*100
            result = approximate_event_date(jd - extra_fuzz, jd + extra_fuzz,
                                            match_finder, match_filter,
                                            distance_function=distance_function,
                                            sample_interval=new_sample_interval,
                                            passes=passes-1)
            if result:
                refined_matches = dict(list(refined_matches.items()) + list(result.items()))
            else:
                if debug_event_approximation:
                    print('Notice: stopping event date approximation with %d passes ' 'remaining.' % (passes-1))
                if match_filter(value) == False:
                    if debug_event_approximation:
                        print('Notice: discarding match %f -> %f:' % (jd, value))
                    continue
                refined_matches[jd] = value
        else:
            if match_filter(value) == False:
                if debug_event_approximation:
                    print('Notice: discarding match %f -> %f:' % (jd, value))
                continue
            refined_matches[jd] = value

    return refined_matches

