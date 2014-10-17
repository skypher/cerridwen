import swisseph as sweph
from cerridwen import jd_now


sweph.set_ephe_path("/home/sky/cerridwen/sweph/")

signs = ['Aries','Taurus','Gemini','Cancer','Leo','Virgo',
         'Libra','Scorpio','Sagittarius','Capricorn','Aquarius','Pisces'];

ranges = [['28 Gem', '10 Can'], ['22 Leo', '28 Leo']]

def sign_abbrev_to_idx(sa):
    abbrevs = [s[0:3] for s in signs]
    for i, abbrev in enumerate(abbrevs):
        if abbrev == sa:
            return i

assert(sign_abbrev_to_idx('Gem')==2)


def absrange(relrange):
    start, end = relrange
    deg, sign = start.split(' ')
    start_abs = int(deg) + sign_abbrev_to_idx(sign)*30
    deg, sign = end.split(' ')
    end_abs = int(deg) + sign_abbrev_to_idx(sign)*30
    return [start_abs, end_abs]


ranges = list(map(absrange, ranges))

def check_bodies_in_any_range(bodies, ranges):
    for id in bodies:
        try:
            long = sweph.calc_ut(jd_now(), id, sweph.FLG_SWIEPH)[0]
        except sweph.Error as e:
            print("id %d doesn't exist: %s" % (id, e))
            next
        for range in ranges:
            if long > range[0] and long < range[1]:
                sign_idx = int(long / 30)
                deg = long - sign_idx*30
                print("%d %s: %d %s" % (id-sweph.AST_OFFSET, sweph.get_planet_name(id), deg, signs[sign_idx]))

print("\nPlanets:")
check_bodies_in_any_range(range(0, sweph.NPLANETS), ranges)

print("\nFictitious Planets:")
fict_ids = range(sweph.FICT_OFFSET, sweph.FICT_OFFSET+sweph.NFICT_ELEM)
check_bodies_in_any_range(fict_ids, ranges)

print("\nAsteroids:")
ast_start = 1000 # starting from 1
ast_end = 2000
ast_ids = range(sweph.AST_OFFSET+ast_start, sweph.AST_OFFSET+ast_end)
check_bodies_in_any_range(ast_ids, ranges)
