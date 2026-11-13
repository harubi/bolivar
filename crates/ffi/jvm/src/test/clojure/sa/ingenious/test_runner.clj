(ns sa.ingenious.test-runner
  "Run every Clojure test namespace; exit non-zero on any failure or error."
  (:require [clojure.test :as t]))

(def namespaces '[sa.ingenious.golden-test sa.ingenious.pdf-test])

(defn -main [& _]
  (apply require namespaces)
  (let [{:keys [fail error]} (apply t/run-tests namespaces)]
    (shutdown-agents)
    (System/exit (if (zero? (+ fail error)) 0 1))))
