# Identifying Key DataRefs for Pilot Performance

##### Preliminary Notes
1. Automatic type conversion is NOT done for you. You need to know the exact data types of the data you reference.
2. Data types are found as a SET which means you can choose which one you want to use, i.e. double or float.
3. Detailed DataRef information can be found at the following links:
    - "https://www.siminnovations.com/xplane/dataref/?name=&type=&writable=&units=&description=&submit=Search"
    - "https://developer.x-plane.com/datarefs/"
4. The values listed may not be the ones we actually want for our plugin, as these all correspond to the true flightmodel readings. There is an argument that DataRefs from gauges and instruments (which may be faulty in the simulator purposefully/accidentally) should be read instead to provide the data the pilot using to fly. I plan to discuss this with my group on Monday during our code reviews and confirm.
5. I am unsure what the github issue meant by finding the usage of these DataRefs. Am I to figure out what they are used for within XPlane itself? Or do I need to figure out what we will be using them for in our plugin? I plan on also raising this issue with my group during our Monday capstone meeting.

## DataRefs

### Altitude
#### Name
1. "sim/flightmodel/position/elevation"

#### Purpose
1. The elevation above MSL (mean sea level) of the aircraft
    - data units: m (meters)

#### DataType
1. double (read only)

#### Usage
tbd


### Airspeed

#### Name
1. "sim/flightmodel/position/indicated_airspeed"
2. "sim/flightmodel/position/indicated_airspeed2"
3. "sim/flightmodel/position/equivalent_airspeed"
4. "sim/flightmodel/position/true_airspeed"

#### Purpose
1. Air speed indicated - this takes into account air density and wind direction
    - data units: kias (knots-indicated airspeed)
2. Air speed indicated - this takes into account air density and wind direction
    - I believe this is just a duplicate of DataRef "sim/flightmodel/position/indicated_airspeed"
    - data units: kias (knots-indicated airspeed)
3. Air speed - equivalent airspeed - this takes compressibility into account
    - data units: keas (knots-equivalent airspeed)
4. Air speed true - this does not take into account air density at altitude!
    - data units: m/s (meters per second)

#### DataType
1. float (read and write)
2. float (read and write)
3. float (read and write)
4. float (read and write)

#### Usage
tbd


### Vertical Airspeed

#### Name
1. "sim/flightmodel/position/vh_ind"
2. "sim/flightmodel/position/vh_ind_fpm"
3. "sim/flightmodel/position/vh_ind_fpm2"

#### Purpose
1. VVI (vertical velocity in meters per second)
2. VVI (vertical velocity in feet per minute)
3. VVI (vertical velocity in feet per minute)
    - I believe this is just a duplicate of DataRef "sim/flightmodel/position/vh_ind_fpm"

#### DataType
1. float (read only)
2. float (read and write)
3. float (read and write)

#### Usage
tbd


### Heading

#### Name
1. "sim/flightmodel/position/mag_psi"
2. "sim/flightmodel/position/psi"
3. "sim/flightmodel/position/true_psi"

#### Purpose
1. The real magnetic heading of the aircraft - the old magpsi dataref was FUBAR
    - data type: degrees
2. The true heading of the aircraft in degrees from the Z axis - OpenGL coordinates
    - data type: degrees
3. The heading of the aircraft relative to the earth precisely below the aircraft - true degrees north, always
    - data type: degrees

#### DataType
1. float (read only)
2. float (read and write)
3. float (read only)

#### Usage
tbd