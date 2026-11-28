# Execution matrix
Dependencies are gates, not a requirement to serialize unrelated research. Wave 0 F001/F002 before implementation. No wave marked done until its own and merged-candidate acceptance.
| Wave | Scope | IDs | Depends on accepted interfaces |
|---|---|---|---|
| 0 | Platform | F001–F012 | Existing R014, auth, Core, 073/074 alignment |
| 1 | GIS Professional | PLG-001–PLG-010 | 0 |
| 2 | AI & Autonomous GIS | PLG-011–PLG-020 | 0,1 |
| 3 | No-code & Low-code | PLG-021–PLG-030 | 0,1 |
| 4 | Remote Sensing | PLG-031–PLG-040 | 0,1 |
| 5 | CAD BIM Engineering | PLG-041–PLG-050 | 0,1 |
| 6 | 3D LiDAR Drone | PLG-051–PLG-060 | 0,1,4 |
| 7 | Agriculture | PLG-061–PLG-070 | 0,1,3;4 imagery optional |
| 8 | Infrastructure | PLG-071–PLG-080 | 0,1,3;5 hydraulic optional |
| 9 | Land Cadastral Planning | PLG-081–PLG-090 | 0,1,3 |
| 10 | Smart City Government | PLG-091–PLG-100 | 0,1,3,9 address/parcel interfaces |
| 11 | Environment | PLG-101–PLG-110 | 0,1,4,6 terrain |
| 12 | Transport Commerce | PLG-111–PLG-120 | 0,1,3 network |
| 13 | Mobile Survey Field | PLG-121–PLG-130 | 0,3,073/074 host qualification |
| 14 | Enterprise Integration | PLG-131–PLG-140 | 0,1, existing integration contracts |
| 15 | Vietnam Professional | PLG-141–PLG-150 | 0,1,9,10,13 relevant localization interfaces |
## Ownership
Agent A owns foundation contracts/migrations; B–P own wave implementations; integration owner resolves shared changes; separate reviewer validates oracle and security. Assign by interface, not by arbitrary maximum agent count. F001 inventory determines exact file paths. All 150 × 4 = 600 plugin tasks, 12 foundation and 4 integration tasks = 616 tracked tasks.