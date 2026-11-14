package sa.ingenious.pdf;

import static com.tngtech.archunit.lang.syntax.ArchRuleDefinition.methods;
import static com.tngtech.archunit.lang.syntax.ArchRuleDefinition.noClasses;

import com.tngtech.archunit.core.domain.JavaClass;
import com.tngtech.archunit.core.domain.JavaMethod;
import com.tngtech.archunit.core.domain.JavaModifier;
import com.tngtech.archunit.core.importer.ImportOption;
import com.tngtech.archunit.junit.AnalyzeClasses;
import com.tngtech.archunit.junit.ArchTest;
import com.tngtech.archunit.lang.ArchCondition;
import com.tngtech.archunit.lang.ArchRule;
import com.tngtech.archunit.lang.ConditionEvents;
import com.tngtech.archunit.lang.SimpleConditionEvent;

/** The public API never hands out the internal native layer. */
@AnalyzeClasses(packages = "sa.ingenious.pdf", importOptions = ImportOption.DoNotIncludeTests.class)
class ArchitectureTest {
  private static final String INTERNAL = "sa.ingenious.pdf.internal";

  private static boolean internal(JavaClass type) {
    return type.getPackageName().equals(INTERNAL);
  }

  private static final ArchCondition<JavaMethod> NOT_EXPOSE_INTERNAL_TYPES =
      new ArchCondition<>("not expose internal types") {
        @Override
        public void check(JavaMethod method, ConditionEvents events) {
          boolean leaks =
              internal(method.getRawReturnType())
                  || method.getRawParameterTypes().stream().anyMatch(ArchitectureTest::internal);
          if (leaks) {
            events.add(SimpleConditionEvent.violated(method, method.getFullName()));
          }
        }
      };

  @ArchTest
  static final ArchRule publicMethodsHideTheNativeLayer =
      methods()
          .that()
          .areDeclaredInClassesThat()
          .resideInAPackage("sa.ingenious.pdf")
          .and()
          .areDeclaredInClassesThat()
          .arePublic()
          .and()
          .arePublic()
          .should(NOT_EXPOSE_INTERNAL_TYPES);

  @ArchTest
  static final ArchRule publicClassesDoNotExtendInternalOnes =
      noClasses()
          .that()
          .resideInAPackage("sa.ingenious.pdf")
          .and()
          .haveModifier(JavaModifier.PUBLIC)
          .should()
          .beAssignableTo(
              com.tngtech.archunit.base.DescribedPredicate.describe(
                  "an internal type", ArchitectureTest::internal));

  @ArchTest
  static final ArchRule theNativeLayerDoesNotUseThePageApi =
      noClasses()
          .that()
          .resideInAPackage(INTERNAL)
          .should()
          .dependOnClassesThat()
          .haveSimpleNameEndingWith("Page");
}
