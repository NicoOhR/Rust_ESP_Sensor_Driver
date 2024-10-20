Sensor driver based on the esp32-s3, currently aiming to support a Hall
effect sensor and a linear potentiometer and spit back to the CAN bus.


## Testing
- [] ADC 
    - [] with potentiometer
    - [] with chosen linpot
    - [] with external adc
- [] CAN/TWAI
    - [x] Self test 
    - [] with transceiver
    - [] on the Pi
- [] PCNT
    - [] with arbitrary digital signal
    - [] with wheel speed sensor
- [] Filtering
    - [] highkey idk
