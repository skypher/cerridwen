# Five common tasks

`curl` recipes against a server running on `127.0.0.1:2828`.
Replace the host as appropriate.

## 1. Where is the Sun / Moon right now?

```bash
curl 'http://127.0.0.1:2828/v1/sun'
curl 'http://127.0.0.1:2828/v1/moon'
```

Both return JSON with `position` (sign, deg, min, sec), the next
ingress event, and rise/set if you pass `latitude` and `longitude`:

```bash
curl 'http://127.0.0.1:2828/v1/sun?latitude=52.5&longitude=13.4'
```

## 2. When is the next full moon? Next eclipse?

The Moon endpoint already returns `next_full_moon` and
`next_new_moon`. For eclipses:

```bash
# Next 8 eclipses (any kind) within ~2 years.
curl 'http://127.0.0.1:2828/v1/eclipses?lookahead=730&limit=8'

# Solar only:
curl 'http://127.0.0.1:2828/v1/eclipses?type=solar&lookahead=730'
```

## 3. What's transiting my natal chart?

```bash
curl 'http://127.0.0.1:2828/v1/transits?natal_date=1985-04-12T15:30:00&tz=America/Chicago&orb=2'
```

For Ascendant/MC aspects too:

```bash
curl 'http://127.0.0.1:2828/v1/transits?natal_date=1985-04-12T15:30:00&tz=America/Chicago&orb=3&include_angles=1&natal_latitude=41.88&natal_longitude=-87.63'
```

## 4. House cusps for a moment + place

```bash
curl 'http://127.0.0.1:2828/v1/houses?latitude=52.5&longitude=13.4&house_system=W'
```

`house_system` accepts letter codes (P/K/W/O/R/C/A/V/M/T/B/Y/X/H/N/D)
or names (`placidus`, `whole_sign`, `koch`, …).

## 5. Subscribe to events as a calendar feed

Generate the events DB once:

```bash
cerridwen-event-generator --jd-start 2461165 --jd-end 2461530 --db /var/lib/cerridwen/events.db
```

Then point your calendar app at:

```
http://127.0.0.1:2828/v1/events.ics?planets=Mercury&types=rx,direct
```

The feed updates whenever the underlying DB does.

## Bonus: get a full natal chart in one call

```bash
curl 'http://127.0.0.1:2828/v1/natal-chart?date=1990-06-15T12:00:00&latitude=52.5&longitude=13.4&house_system=P'
```

That response combines house cusps, bodies with house placement,
instantaneous aspects, and Hellenistic lots.

## Bonus: ask in sidereal mode

Add `?zodiac=sidereal&ayanamsha=lahiri` (or `krishnamurti`,
`fagan_bradley`, `raman`, `yukteshwar`, …) to any of the above:

```bash
curl 'http://127.0.0.1:2828/v1/sun?zodiac=sidereal&ayanamsha=lahiri'
curl 'http://127.0.0.1:2828/v1/body/jupiter?zodiac=sidereal&ayanamsha=krishnamurti'
```
