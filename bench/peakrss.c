/* Run a command and print its peak resident memory (KB). A tiny C launcher keeps
   its own footprint (~1 MB) out of the measurement, unlike a Python launcher. */
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <fcntl.h>
int main(int argc, char **argv) {
    pid_t pid = fork();
    if (pid == 0) { int fd = open("/dev/null", O_WRONLY); dup2(fd, 1); dup2(fd, 2); execvp(argv[1], argv + 1); _exit(127); }
    int status; struct rusage ru; wait4(pid, &status, 0, &ru);
    printf("%ld\n", ru.ru_maxrss);
    return WEXITSTATUS(status);
}
