# Experiment setting
So, I have completed some prog generation algorithms and prog execution modules by now. As a simple integration test, I wrote a very simple fuzz driver that doesn't use any feedback analysis, and then used the driver to test the four versions of the kernel for 24 hours. 
As a result, the driver crashed the kernel hundreds of times and found dozens of effective vulnerabilities, which looks great.

# Partial Result
Confusion of data makes integration difficult. The rough statistics are shown below.

| kernel version | bug number |
|----------------|------------|
| 5.10           | 18+        |
| 5.7            | 4+         |
| 5.6-KTSAN      | 5+         |
| 4.20           | 2+         |


# Conclusion
1. Generation algorithms and execution modules works.
2. Need more effective fuzzer.
3. Need better crash triage mechanism.
