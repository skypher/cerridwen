import os
import swisseph as sweph


### FILES AND DIRECTORIES
_ROOT = os.path.abspath(os.path.dirname(__file__))
sweph_dir = os.path.join(_ROOT, '../sweph')
dbfile = os.path.join(_ROOT, 'events.db')

sweph.set_ephe_path(sweph_dir)


### APPROXIMATOR TWEAKS
debug_event_approximation = False

maximum_error = 2e-6 # our guaranteed maximum error
max_data_points = 100000


### ASPECTS
# TODO there are quite a few minor aspects that aren't included yet.
dexter_aspects = [
        (30, 'semi-sextile'),
        (45, 'semi-square'),
        (60, 'sextile'),
        (72, 'quintile'),
        (120, 'trine'),
        (144, 'bi-quintile'),
        (150, 'quincunx')
]

sinister_aspects = reversed([(360 - x[0], x[1]) for x in dexter_aspects])

aspects = (
        [(0, 'conjunction', None)] +
        [x + ('dexter',) for x in dexter_aspects] +
        [(180, 'opposition', None)] +
        [x + ('sinister',) for x in sinister_aspects]
)

traditional_major_aspects = ['conjunction', 'sextile', 'square', 'trine', 'opposition']
sign_related_aspects = ['conjunction', 'semi-sextile', 'sextile', 'square', 'trine', 'quincunx', 'opposition']

signs = ['Aries','Taurus','Gemini','Cancer','Leo','Virgo',
         'Libra','Scorpio','Sagittarius','Capricorn','Aquarius','Pisces'];

time_scale = 'tt' # recommended by the IAU for Julian days
