/* binary-trees: builds and walks many complete binary trees, as the Computer Language Benchmarks
 * Game writes it. Allocation with malloc and free. The argument is the depth of the largest. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct Node {
  struct Node *left;
  struct Node *right;
} Node;

static Node *tree(unsigned depth) {
  Node *node = malloc(sizeof(Node));
  if (node == NULL) {
    abort();
  }
  node->left = depth == 0 ? NULL : tree(depth - 1);
  node->right = depth == 0 ? NULL : tree(depth - 1);
  return node;
}

/* The number of nodes of the tree. */
static uint64_t check(const Node *node) {
  return 1 + (node->left != NULL ? check(node->left) : 0) + (node->right != NULL ? check(node->right) : 0);
}

static void destroy(Node *node) {
  if (node->left != NULL) {
    destroy(node->left);
  }
  if (node->right != NULL) {
    destroy(node->right);
  }
  free(node);
}

int main(int argc, char **argv) {
  unsigned requested = argc > 1 ? (unsigned)strtoul(argv[1], NULL, 10) : 10;
  unsigned min_depth = 4;
  unsigned max_depth = requested < min_depth + 2 ? min_depth + 2 : requested;

  Node *stretch = tree(max_depth + 1);
  printf("stretch tree of depth %u\t check: %llu\n", max_depth + 1, (unsigned long long)check(stretch));
  destroy(stretch);

  Node *long_lived = tree(max_depth);
  for (unsigned depth = min_depth; depth <= max_depth; depth += 2) {
    uint64_t iterations = (uint64_t)1 << (max_depth - depth + min_depth);
    uint64_t total = 0;
    for (uint64_t i = 0; i < iterations; i++) {
      Node *t = tree(depth);
      total += check(t);
      destroy(t);
    }
    printf("%llu\t trees of depth %u\t check: %llu\n", (unsigned long long)iterations, depth, (unsigned long long)total);
  }

  printf("long lived tree of depth %u\t check: %llu\n", max_depth, (unsigned long long)check(long_lived));
  destroy(long_lived);
  return 0;
}
