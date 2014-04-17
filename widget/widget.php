<html>
<head>
<style type="text/css">
  @import url(http://reset5.googlecode.com/hg/reset.min.css);
  @font-face
  {
    font-family: HamburgSymbols;
    src: url(/HamburgSymbols.ttf);
  }
  @font-face
  {
    font-family: IntellectaCrowns;
    src: url(/IntellectaCrowns.ttf);
  }

  span.glyph { font-family: HamburgSymbols; }
  span.glyph.hamburg.aries:before { content:"a"; }
  span.glyph.hamburg.taurus:before { content:"s"; }
  span.glyph.hamburg.gemini:before { content:"d"; }
  span.glyph.hamburg.cancer:before { content:"f"; }
  span.glyph.hamburg.leo:before { content:"g"; }
  span.glyph.hamburg.virgo:before { content:"n"; }
  span.glyph.hamburg.libra:before { content:"j"; }
  span.glyph.hamburg.scorpio:before { content:"k"; }
  span.glyph.hamburg.sagittarius:before { content:"l"; }
  span.glyph.hamburg.capricorn:before { content:"v"; }
  span.glyph.hamburg.aquarius:before { content:"x"; }
  span.glyph.hamburg.pisces:before { content:"c"; }

  div.dignity span.glyph.crown {
      font-family:IntellectaCrowns;
      font-size:30px;
  }

  div.dignity span.glyph.crown.rulership,
  div.dignity span.glyph.crown.exaltation
  {
      color:yellow;
  }
  div.dignity span.glyph.crown.rulership:before,
  div.dignity span.glyph.crown.exaltation:before
  {
      content:"c";
  }

  body { background:black; color:#f0ecff; text-align:center; margin:15px 0;}

  .illumination, .speed, .diameter-ratio { display:none; }

  .image { margin:15px 0; }
</style>
</head>
<body>
<pre>
<?php
$moon_json_uri = 'http://localhost:5000/json';
$json = file_get_contents($moon_json_uri);
$data = json_decode($json, true);
print_r($data);
?>
</pre>
<div class="moon-widget">
    <div class="time utc">{{ data.jd|prettytime }}</div>
    <div class="sign">{{ '%d' % data.position.degrees }} <span class="glyph hamburg {{ data.position.sign|lower }}" title="{{ data.position.sign }}"></span>
        {{ '%d' % data.position.minutes }}'</div>
    <div class="phase">{{ data.phase.trend }} {{ data.phase.shape }}</div>
    <div class="dignity">
        <span class="glyph crown dignity {{ data.dignity|lower }}" title="{{ data.dignity }}"></span>
    </div>
    <div class="image">
      <img src="fullmoon.jpg" width="100" height="100">
    </div>
    <div class="illumination">{{ (data.illumination * 100) // 1 }}% illuminated</div>
    <div class="speed">
      Speed: {{ '%d' % (data.speed_ratio * 100) }}%
      ({{ '%.1f' % data.speed }} deg/d)</div>
    <div class="diameter-ratio">Apparent Diameter: {{ '%d' % (data.diameter_ratio * 100) }}%</div>
    <div class="next-event">
        {{ data.next_new_full_moon.description }} <br>
        in {{ data.next_new_full_moon.delta_jd|render_delta_jd }}
    </div>
</div>
</body>
</html>
