# PHP baseline (starting HEAD 07791c0)

PHP extraction already covers: namespaces, classes/interfaces/traits/enums,
functions/methods/constructors, properties, constants, use/use-function/
use-const/grouped imports + aliases, inheritance/interface references,
trait-composition references, plain/member/nullsafe/scoped/construction/
indirect calls, includes, first-class callables, test evidence, ranges,
recovery. Structural links: php.namespace.* / php.use.* (syntactic qualified
names; no autoload assumptions).

Baseline gap: NO candidate layer — every PHP call produced `no_candidate_rule`
disposition; callers/callees returned no PHP call evidence.
