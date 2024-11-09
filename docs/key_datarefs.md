# Identifying Key DataRefs for Pilot Performance


## Notes and Important Information
1. Automatic type conversion is NOT done for you. This means you need to know the EXACT data types, i.e. float or int, of the data you reference, as well as the type's respective "setter" and "getter" functions. DataRefs also often come as a set, so there can be multiple data types for each reference data, such as having the same altitude value available as a double or float DataRef.

2. The DataRefs listed in this document are mainly those of which the pilot and copilot are reading in the cockpit, i.e. from the primary flight display. This means that these readings MAY OR MAY NOT be incorrect due to sensors, gyroscopes, etc. giving incorrect readings. As this project is focused on collecting data on the pilot's performance, these DataRefs are not the objective flight positions (though we hope the sensors are accurate enough for this to be the case either way).

3. Because the DataRefs in this documentation refer to the sensors of the airplane rather than the physical position of the flightmodel, you may want to change the DataRefs in this folder "sim/flightmodel2/*" instead if you wish to change data relating to the physical position of the aircraft, i.e. changing the indicated airspeed of the aircraft, not the sensor, with DataRef "sim/flightmodel2/position/indicated_airspeed". This will probably not come up in this project, however I felt it prudent to discuss.

4. These DataRefs are primarily found in the "sim/cockpit2" folder as these are the most up-to-date and modern DataRefs for XPlane. "sim/cockpit" is a legacy folder for older versions of the simulator (though I am unsure if it still recieves updates or not). The same holds true for "sim/flightmodel2" and any other applicable folders of XPlane DataRefs.

5. Detailed DataRef information can be found at the following link:
    - "https://developer.x-plane.com/datarefs/" (the official dev website)



## DataRefs


### Altitude

#### Name
1. "sim/cockpit2/gauges/indicators/altitude_ft_pilot"
2. "sim/cockpit2/gauges/indicators/altitude_ft_copilot"

#### Purpose
1. Indicated height, MSL (mean sea level), in feet, primary system, based on pilots barometric pressure input.
    - data units: ft above MSL (mean sea level)
2. Indicated height, MSL (mean sea level), in feet, primary system, based on co-pilots barometric pressure input.
    - data units: ft above MSL (mean sea level)

#### DataType
1. float (read/write)
2. float (read/write)

#### Usage
1. This is the primary indicator to see the plane's altitude for the pilot. 


### Airspeed

#### Name
1. "sim/cockpit2/gauges/indicators/true_airspeed_kts_pilot"
2. "sim/cockpit2/gauges/indicators/true_airspeed_kts_copilot"
3. "sim/cockpit2/gauges/indicators/airspeed_kts_pilot"
4. "sim/cockpit2/gauges/indicators/airspeed_kts_copilot"

#### Purpose
1. True airspeed in knots, for pilot pitot/static, calculated by ADC, requires pitot, static, oat sensor and ADC all to work correctly to give correct value
    - data units: kts (knots) 
    - [A knot is one nautical mile/hr, 1.15 m/hr, or 1.85 km/hr]
2. True airspeed in knots, for copilot pitot/static, calculated by ADC, requires pitot, static, oat sensor and ADC all to work correctly to give correct value
    - data units: kts (knots) 
    - [A knot is one nautical mile/hr, 1.15 m/hr, or 1.85 km/hr]
3. Indicated airspeed in knots, pilot. 
    - data units: kts (knots) 
    - [A knot is one nautical mile/hr, 1.15 m/hr, or 1.85 km/hr]
    - Writeable with "override_IAS"
4. Indicated airspeed in knots, copilot. 
    - data units: kts (knots) 
    - [A knot is one nautical mile/hr, 1.15 m/hr, or 1.85 km/hr]
    - Writeable with "override_IAS"

#### DataType
1. float (read/write)
2. float (read/write)
3. float (read/write)
4. float (read/write)

#### Usage
1. True Airspeed is the airspeed shown on the airspeed indicator corrected for position installion error compressibility, temperature, and pressure altitude. It's basically how fast you are actually moving through the air. It is shown on the electronic flight display
    - Airspeed Flow Chart: Indicated Airspeed (airspeed) --> Calibrated Airspeed (position install error) --> Equivalent Airspeed (compressibility) --> True Airspeed (temperature and pressure altitude)
2. True Airspeed but for the copilot's EFD
3. Indicated airspeed measures dynamic pressure and is the direct airspeed measurement from the plane. It's what speed your aircraft would feel like it's flying if it were at sea level. Indicated Airspeed is shown on the Airspeed Indicator and on the EFD.
4. Indicated Airspeed but for the copilot's EFD


### Vertical Airspeed

#### Name
1. "sim/cockpit2/gauges/indicators/vvi_fpm_pilot"
2. "sim/cockpit2/gauges/indicators/vvi_fpm_copilot"

#### Purpose
1. Indicated vertical speed in feet per minute, pilot system.
    - data units: ft/min (FPM)
2. Indicated vertical speed in feet per minute, copilot system.
    - data units: ft/min (FPM)

#### DataType
1. float (read/write)
2. float (read/write)

#### Usage
1. VVI stands for "vertical velocity indicator" and is the instrument that displays the vertical airspeed in the cockpit. This is the masure of how fast the airplane is in the vertical direction, and can accordingly be a positive or negative value (positive for upwards, negative for downwards). This DataRef corresponds to the pilot's VVI in the cockpit.
2. This is the same, but for the copilot's VVI.


### Heading

#### Name
1. "sim/cockpit2/gauges/indicators/heading_AHARS_deg_mag_pilot"
2. "sim/cockpit2/gauges/indicators/heading_AHARS_deg_mag_copilot"

#### Purpose
1. Indicated magnetic heading, in degrees. Source: AHARS. Side: Pilot
    - data type: degrees_magnetic (ºM)
2. Indicated magnetic heading, in degrees. Source: AHARS. Side: Copilot
    - data type: degrees_magnetic (ºM)

#### DataType
1. float (read/write)
2. float (read/write)

#### Usage
1. AHARS Magnetic Heading is used as the primary indication of heading and altitude for the pilot and integrated within the cockpit's primary flight display on the pilot's side.
    - Definition of AHARS: "An attitude and heading reference system (AHRS) consists of sensors on three axes that provide attitude information for aircraft, including roll, pitch, and yaw. These are sometimes referred to as MARG (Magnetic, Angular Rate, and Gravity) sensors and consist of either solid-state or microelectromechanical systems (MEMS) gyroscopes, accelerometers and magnetometers. They are designed to replace traditional mechanical gyroscopic flight instruments." (Wikipedia)
2. AHARS Magnetic Heading but for the copilot's indicator in the cockpit.