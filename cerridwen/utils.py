import astropy
import math
from .defs import time_scale

def jd_now():
    return astropy.time.Time.now().jd

def iso2jd(iso):
    return astropy.time.Time(iso, scale=time_scale).jd
                        
def jd2iso(jd):
    """Convert a Julian date into an ISO 8601 date string representation"""
    return astropy.time.Time(jd, format='jd', scale=time_scale, precision=0).iso

def parse_jd_or_iso_date(date):
    for format in ['jd', 'iso', 'isot']:
        try:
            return astropy.time.Time(float(date) if format == 'jd' else date,
                    format=format, scale=time_scale).jd
        except ValueError:
            continue
    raise ValueError('Please pass the date as either a Julian Day decimal string ' +
                     '(e.g. "2456799.9897") or as an ISO8601 string denoting a UTC ' +
                     'time in this format: 2014-05-20 23:37:17.')

def mod360_distance(a, b):
    """distance between a and b in mod(360)"""
    # Surprisingly enough there doesn't seem to be a more elegant way.
    # Check http://stackoverflow.com/questions/6192825/
    a %= 360
    b %= 360

    if a < b:
        return mod360_distance(b, a)
    else:
        return min(a-b, b-a+360)

def days_frac_to_dhms(days_frac):
    """Convert a day float to integer days, hours, minutes and seconds.

    Returns a tuple (days, hours, minutes, seconds).
    
    >>> days_frac_to_dhms(2.5305)
    (2, 12, 43, 55)
    """
    days = math.floor(days_frac)
    hms_frac = days_frac - days
    hours = math.floor(hms_frac * 24)
    minutes_frac = hms_frac - hours / 24
    minutes = math.floor(minutes_frac * 1440)
    seconds = math.floor((minutes_frac - minutes / 1440) * 86400)

    return (days, hours, minutes, seconds)

def render_pretty_time(jd):
    """Convert jd into a pretty string representation"""
    year, month, day, hour_frac = sweph.revjul(jd)
    _, hours, minutes, seconds = days_frac_to_dhms(hour_frac/24)
    time_ = calendar.timegm((year,month,day,hours,minutes,seconds,0,0,0))
    return time.strftime('%e %b %Y %H:%M UTC', time.gmtime(time_))

def render_delta_days(delta_days):
    """Convert a time delta into a pretty string representation"""
    days, hours, minutes = days_frac_to_dhms(delta_days)[:3]
    result = [] 

    if days > 0:
        result += ['%d days' % days]

    if hours > 0:
        result += ['%d hours' % hours]

    if days == 0 and minutes > 0:
        result += ['%d minutes' % minutes]

    if days == 0 and hours == 0 and minutes == 0:
        result = ['less than a minute']

    return ' '.join(result);

def angle_to_aspect_name(angle):
    return [aspect[1] for aspect in aspects if aspect[0] == angle]

def aspect_name_to_angle(name):
    "Get the (dexter, if applicable) angle for an aspect name"
    if name == 'conjunction':
        return 0
    if name == 'opposition':
        return 180
    else:
        return [aspect[0] for aspect in dexter_aspects if aspect[1] == name]
                   
