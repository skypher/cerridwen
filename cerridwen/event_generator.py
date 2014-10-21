
from cerridwen import defs
from .utils import jd2iso
from .planets import Moon, Sun, Mercury, Venus, Mars, Jupiter, Saturn, Uranus, Neptune, Pluto

def generate_event_table(jd_start, jd_end, planets=None, aspects=None,
                         compute_aspects=True, compute_ingresses=True, compute_retrogrades=True):
    # TODO this generates events that slightly exceed jd_end;
    # reject events that are beyond the requested period in pump_events() to fix this.
    if planets is None:
        planets = [Moon(), Sun(), Mercury(), Venus(), Mars(), Jupiter(), Saturn()]
    if aspects is None: aspects = defs.aspects

    import sqlite3
    
    conn = sqlite3.connect(defs.dbfile)

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

    # aspects
    #aspects = [(72, 'quintile', 'dexter'), (288, 'quintile', 'sinister')]
    #planets = [Venus(), Mars()]
    #aspects = [(30, 'sextile', 'dexter')]
    if compute_aspects:
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
    if compute_ingresses:
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
    for planet in planets:
        if planet.name() not in ['Moon', 'Sun']:
            def event_function(jd):
                event = planet.next_rx_event(jd)
                event_jd = event['jd']
                event_type = event['type']
                event_subtype = None
                event_planet = planet.name()
                event_data = planet.sign(event_jd)
                return (event_jd, event_type, event_subtype, event_planet, event_data)
            pump_events(event_function)

    conn.commit()

    conn.close()

