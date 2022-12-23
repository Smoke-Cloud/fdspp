# FDS Pre-Processor

![build checks](https://github.com/Smoke-Cloud/fdspp/actions/workflows/checks.yaml/badge.svg)

`fdspp` is a pre-processor for [FDS](https://github.com/firemodels/fds)

One of the primary purposes is to automatically distribute meshes to MPI process
depending on their size. For example, do distribute meshes amongst 4 MPI process
run the following:

```sh
$ fdspp input.fds new-input.fds --n-mpi-4 # Allocate the meshes
MPI Mesh Allocation
  MPI_PROCESS 0: TOTAL: 1476096 [1476096]
  MPI_PROCESS 1: TOTAL: 1476096 [1476096]
  MPI_PROCESS 2: TOTAL: 1127744 [403200, 201600, 162944, 134400, 128000, 44000, 32000, 21600]
  MPI_PROCESS 3: TOTAL: 1139552 [352000, 246400, 172800, 131200, 81472, 59904, 33600, 30800, 16400, 14976]
MPI Cell Count Variation: +/- 13.38
$ mpiexec -np 4 fds new-input.fds # Run the modified input file
```
