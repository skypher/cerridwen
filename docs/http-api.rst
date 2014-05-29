HTTP API
========

.. http:get:: /v1/sun

   Get data on the Sun.

   :query latitude: observer's latitude (decimal, optional)
   :query longitude: observer's longitude (decimal, optional)
   :statuscode 200: success
   :statuscode 400: value error; one of the parameters passed
                    couldn't be parsed.

   **Notes**
   
   * The response will include rise and set times only if both *latitude* and
     *longitude* are specified.

   * *latitude* must be between and including -90 and 90.

   * *longitude* must be between and including -180 and 180.

   * The format of *latitude* and *longitude* must be something parsable
     by Python's :py:func:`float` function.

   * It is an error to specify only one of *latitude* and *longitude*

   **Example request**:

   Get sun data for the current moment, including rise and set times for Berlin.

   URI: http://cerridwen.viridian-project.de/api/v1/sun?latitude=52.5&longitude=13.3
   
   .. sourcecode:: javascript

      {
            "jd": 2456805.9347222224, 
            "iso_date": "2014-05-28T10:26:00Z", 
            "position": {
                    "absolute_degrees": 67.02621001184063, 
                    "sign": "Gemini", 
                    "deg": 7.026210011840632, 
                    "min": 1.5726007104379391, 
                    "rel_tuple": [
                            "Gemini", 
                            7.026210011840632, 
                            1.5726007104379391
                    ]
            }, 
            "dignity": null, 
            "next_rise": {
                    "description": "Sun rises", 
                    "jd": 2456806.6199322133, 
                    "iso_date": "2014-05-29T02:52:42Z", 
                    "delta_days": 0.6852099909447134
            }, 
            "next_set": {
                    "description": "Sun sets", 
                    "jd": 2456806.3022005144, 
                    "iso_date": "2014-05-28T19:15:10Z", 
                    "delta_days": 0.36747829196974635
            }, 
            "last_rise": {
                    "description": "Sun rises", 
                    "jd": 2456805.620638409, 
                    "iso_date": "2014-05-28T02:53:43Z", 
                    "delta_days": -0.3140838132239878
            }, 
            "last_set": {
                    "description": "Sun sets", 
                    "jd": 2456805.301308829, 
                    "iso_date": "2014-05-27T19:13:53Z", 
                    "delta_days": -0.6334133935160935
            }
      } 


.. http:get:: /v1/moon

   Like the sun endpoint, but includes a lot more data that only makes
   sense for the moon.

   **Example request**:

   Get moon data for the current moment, including rise and set times for Berlin.

   URI: http://cerridwen.viridian-project.de/api/v1/sun?latitude=52.5&longitude=13.3
   
   .. sourcecode:: javascript

      {
              "jd": 2456805.935416667, 
              "iso_date": "2014-05-28T10:27:00Z", 
              "position": {
                      "absolute_degrees": 63.00766509063341, 
                      "sign": "Gemini", 
                      "deg": 3.0076650906334095, 
                      "min": 0.4599054380045686, 
                      "rel_tuple": [
                              "Gemini", 
                              3.0076650906334095, 
                              0.4599054380045686
                      ]
              }, 
              "phase": {
                      "trend": "waning", 
                      "shape": "crescent", 
                      "quarter": 0, 
                      "quarter_english": "new"
              }, 
              "illumination": 0.022328953544355084, 
              "distance": 0.002617405829474053, 
              "diameter": 30.52102695101311, 
              "diameter_ratio": 0.2543806147943976, 
              "speed": 12.729377304450301, 
              "speed_ratio": 0.35293040764071915, 
              "age": 29.175456268712878, 
              "period_length": 29.517968974076211, 
              "dignity": null, 
              "next_new_moon": {
                      "description": "Upcoming new moon in Gemini", 
                      "jd": 2456806.2779293722, 
                      "iso_date": "2014-05-28T18:40:13Z", 
                      "delta_days": 0.34251270536333323
              }, 
              "next_full_moon": {
                      "description": "Upcoming full moon in Sagittarius", 
                      "jd": 2456821.6746404273, 
                      "iso_date": "2014-06-13T04:11:28Z", 
                      "delta_days": 15.739223760552704
              }, 
              "next_new_or_full_moon": {
                      "description": "Upcoming new moon in Gemini", 
                      "jd": 2456806.2779293722, 
                      "iso_date": "2014-05-28T18:40:13Z", 
                      "delta_days": 0.34251270536333323
              }, 
              "last_new_moon": {
                      "description": "Preceding new moon in Taurus", 
                      "jd": 2456776.7599603981, 
                      "iso_date": "2014-04-29T06:14:20Z", 
                      "delta_days": -29.175456268712878
              }, 
              "last_full_moon": {
                      "description": "Preceding full moon in Scorpio", 
                      "jd": 2456792.3027133634, 
                      "iso_date": "2014-05-14T19:15:54Z", 
                      "delta_days": -13.632703303359449
              }, 
              "next_rise": {
                      "description": "Moon rises", 
                      "jd": 2456806.653334031, 
                      "iso_date": "2014-05-29T03:40:48Z", 
                      "delta_days": 0.7179173640906811
              }, 
              "next_set": {
                      "description": "Moon sets", 
                      "jd": 2456806.2835339396, 
                      "iso_date": "2014-05-28T18:48:17Z", 
                      "delta_days": 0.34811727283522487
              }, 
              "last_rise": {
                      "description": "Moon rises", 
                      "jd": 2456805.624089608, 
                      "iso_date": "2014-05-28T02:58:41Z", 
                      "delta_days": -0.3113270588219166
              }, 
              "last_set": {
                      "description": "Moon sets", 
                      "jd": 2456805.2403595136, 
                      "iso_date": "2014-05-27T17:46:07Z", 
                      "delta_days": -0.6950571532361209
              }
      }
